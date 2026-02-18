use std::sync::Arc;

use tokio::sync::Mutex;

use crate::config::{get_table_name, ConfigState};
use crate::events::{AppEvent, EventSender};
use crate::indexer;
use crate::state::{
    ContainerListItem, DbState, ModelState, RerankerState, SearchResult,
};
use crate::watcher;

pub async fn get_containers(
    config_state: &ConfigState,
) -> Result<(Vec<ContainerListItem>, String), String> {
    let config = config_state.config.lock().await;
    let list: Vec<ContainerListItem> = config
        .containers
        .iter()
        .map(|(name, info)| ContainerListItem {
            name: name.clone(),
            description: info.description.clone(),
            indexed_paths: info.indexed_paths.clone(),
        })
        .collect();
    Ok((list, config.active_container.clone()))
}

pub async fn create_container(
    name: String,
    description: String,
    config_state: &ConfigState,
) -> Result<(), String> {
    let mut config = config_state.config.lock().await;
    if config.containers.contains_key(&name) {
        return Err("Container already exists".to_string());
    }
    config.containers.insert(
        name,
        crate::config::ContainerInfo {
            description,
            indexed_paths: Vec::new(),
        },
    );
    drop(config);
    config_state.save().await?;
    Ok(())
}

pub async fn delete_container(
    name: String,
    config_state: &ConfigState,
    db_state: &Arc<Mutex<DbState>>,
) -> Result<(), String> {
    {
        let mut config = config_state.config.lock().await;
        if name == "Default" {
            return Err("Cannot delete Default container".to_string());
        }
        if config.active_container == name {
            config.active_container = "Default".to_string();
        }
        config.containers.remove(&name);
    }

    config_state.save().await?;

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };
    let table_name = get_table_name(&name);
    let _ = db.drop_table(&table_name, &[]).await;

    Ok(())
}

pub async fn set_active_container(
    name: String,
    config_state: &ConfigState,
    db_state: &Arc<Mutex<DbState>>,
    model_state: &Arc<Mutex<ModelState>>,
    watcher_state: &watcher::WatcherState,
    tx: EventSender,
) -> Result<(), String> {
    let mut config = config_state.config.lock().await;
    if !config.containers.contains_key(&name) {
        return Err("Container does not exist".to_string());
    }
    config.active_container = name;
    drop(config);
    config_state.save().await?;

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };
    watcher::restart(watcher_state, config_state, db, model_state.clone(), tx).await;

    Ok(())
}

pub async fn search(
    query: String,
    db_state: &Arc<Mutex<DbState>>,
    model_state: &Arc<Mutex<ModelState>>,
    reranker_state: &Arc<Mutex<RerankerState>>,
    config_state: &ConfigState,
) -> Result<Vec<SearchResult>, String> {
    let table_name = {
        let config = config_state.config.lock().await;
        get_table_name(&config.active_container)
    };

    let query_vector = {
        let mut guard = model_state.lock().await;
        if let Some(err) = &guard.init_error {
            return Err(format!("Model failed to load: {}", err));
        }
        let model = guard
            .model
            .as_mut()
            .ok_or("AI model is loading... Please wait a moment.")?;
        indexer::embed_query(model, &query).map_err(|e| e.to_string())?
    };

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };

    let query_variants = indexer::expand_query(&query);

    let vector_fut =
        indexer::search_files(&db, &table_name, &query_vector, 50, None, None, false);

    let fts_db = db.clone();
    let fts_table_name = table_name.clone();
    let fts_fut = async move {
        let futs: Vec<_> = query_variants
            .iter()
            .map(|variant| indexer::search_fts(&fts_db, &fts_table_name, variant, 30, None, None, false))
            .collect();
        let results = futures::future::join_all(futs).await;
        let mut all_fts: Vec<(String, String)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for result in results.into_iter().flatten() {
            for item in result {
                if seen.insert(item.0.clone()) {
                    all_fts.push(item);
                }
            }
        }
        all_fts
    };

    let (vector_result, fts_results) = tokio::join!(vector_fut, fts_fut);

    let vector_results = vector_result.map_err(|e| e.to_string())?;

    let merged = if fts_results.is_empty() {
        vector_results
    } else {
        indexer::hybrid_merge(&vector_results, &fts_results, 50)
    };

    let rerank_input: Vec<(String, String, f32)> = merged.into_iter().take(15).collect();

    let used_hybrid = !fts_results.is_empty();

    let (final_results, used_reranker) = {
        let reranker_opt = {
            let mut guard = reranker_state.lock().await;
            guard.reranker.take()
        };
        if let Some(reranker) = reranker_opt {
            let (reranker_back, results, used) =
                indexer::safe_rerank(reranker, query.clone(), rerank_input).await;
            {
                let mut guard = reranker_state.lock().await;
                if let Some(r) = reranker_back {
                    guard.reranker = Some(r);
                }
            }
            (results, used)
        } else {
            (rerank_input, false)
        }
    };

    let scored: Vec<SearchResult> = crate::indexer::pipeline::score_results(final_results, used_reranker, used_hybrid, 20)
        .into_iter()
        .map(|r| SearchResult { path: r.path, snippet: r.snippet, score: r.score })
        .collect();

    Ok(scored)
}

pub async fn index_folder(
    dir: String,
    db_state: &Arc<Mutex<DbState>>,
    model_state: &Arc<Mutex<ModelState>>,
    config_state: &ConfigState,
    watcher_state: &watcher::WatcherState,
    tx: EventSender,
) -> Result<String, String> {
    let table_name = {
        let config = config_state.config.lock().await;
        get_table_name(&config.active_container)
    };

    {
        let mut config = config_state.config.lock().await;
        let active = config.active_container.clone();
        if let Some(info) = config.containers.get_mut(&active) {
            if !info.indexed_paths.contains(&dir) {
                info.indexed_paths.push(dir.clone());
            }
        }
        drop(config);
        config_state.save().await?;
    }

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };

    let ms = model_state.clone();
    let progress_tx = tx.clone();

    let indexing_config = {
        let config = config_state.config.lock().await;
        config.indexing.clone()
    };

    let count = indexer::index_directory(
        &dir,
        &table_name,
        &db,
        &ms,
        &indexing_config,
        move |current, total, path| {
            let _ = progress_tx.send(AppEvent::IndexingProgress {
                current,
                total,
                path,
            });
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let _ = tx.send(AppEvent::IndexingComplete(format!(
        "{} files indexed",
        count
    )));

    let db2 = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };
    watcher::restart(watcher_state, config_state, db2, model_state.clone(), tx).await;

    Ok(format!("Indexed {} files", count))
}

pub async fn reset_index(
    db_state: &Arc<Mutex<DbState>>,
    config_state: &ConfigState,
) -> Result<(), String> {
    let table_name = {
        let config = config_state.config.lock().await;
        get_table_name(&config.active_container)
    };

    let path = {
        let guard = db_state.lock().await;
        guard.path.clone()
    };
    indexer::reset_index(&path, &table_name)
        .await
        .map_err(|e| e.to_string())
}

pub async fn reindex_all(
    db_state: &Arc<Mutex<DbState>>,
    model_state: &Arc<Mutex<ModelState>>,
    config_state: &ConfigState,
    tx: EventSender,
) -> Result<String, String> {
    let (table_name, paths) = {
        let config = config_state.config.lock().await;
        let info = config
            .containers
            .get(&config.active_container)
            .ok_or("Active container not found")?;
        (
            get_table_name(&config.active_container),
            info.indexed_paths.clone(),
        )
    };

    if paths.is_empty() {
        return Err("No folders to reindex".to_string());
    }

    let db = {
        let guard = db_state.lock().await;
        guard.db.clone()
    };

    let ms = model_state.clone();

    let indexing_config = {
        let config = config_state.config.lock().await;
        config.indexing.clone()
    };

    let mut total = 0;
    for dir in &paths {
        let progress_tx = tx.clone();
        let count = indexer::index_directory(
            dir,
            &table_name,
            &db,
            &ms,
            &indexing_config,
            move |current, total, path| {
                let _ = progress_tx.send(AppEvent::IndexingProgress {
                    current,
                    total,
                    path,
                });
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        total += count;
    }

    let _ = tx.send(AppEvent::IndexingComplete(format!(
        "{} files reindexed from {} folders",
        total,
        paths.len()
    )));

    Ok(format!(
        "Reindexed {} files from {} folders",
        total,
        paths.len()
    ))
}
