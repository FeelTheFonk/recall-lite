use anyhow::{anyhow, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use fastembed::{RerankInitOptions, RerankResult, RerankerModel, TextRerank};

const QUERY_PREFIX: &str = "query: ";
const PASSAGE_PREFIX: &str = "passage: ";

pub fn load_model(model: EmbeddingModel, cache_dir: std::path::PathBuf) -> Result<TextEmbedding> {
    let mut options = InitOptions::default();
    options.model_name = model;
    options.cache_dir = cache_dir;
    options.show_download_progress = cfg!(debug_assertions);
    TextEmbedding::try_new(options)
}

pub fn load_reranker(cache_dir: std::path::PathBuf) -> Result<TextRerank> {
    let mut options = RerankInitOptions::default();
    options.model_name = RerankerModel::JINARerankerV2BaseMultiligual;
    options.cache_dir = cache_dir;
    options.show_download_progress = cfg!(debug_assertions);
    TextRerank::try_new(options).map_err(|e| anyhow!("Failed to load reranker: {}", e))
}

pub fn embed_passages(model: &mut TextEmbedding, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
    let prefixed: Vec<String> = texts
        .into_iter()
        .map(|t| format!("{}{}", PASSAGE_PREFIX, t))
        .collect();
    model
        .embed(prefixed, None)
        .map_err(|e| anyhow!("Embedding failed: {}", e))
}

pub fn embed_query(model: &mut TextEmbedding, query: &str) -> Result<Vec<f32>> {
    let prefixed = format!("{}{}", QUERY_PREFIX, query);
    let embeddings = model
        .embed(vec![prefixed], None)
        .map_err(|e| anyhow!("Embedding failed: {}", e))?;
    embeddings
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Empty embedding result"))
}

pub fn get_model_dimension(model: &mut TextEmbedding) -> Result<usize> {
    let probe = model
        .embed(vec!["dimension probe".to_string()], None)
        .map_err(|e| anyhow!("Dimension probe failed: {}", e))?;
    probe
        .first()
        .map(|v| v.len())
        .ok_or_else(|| anyhow!("No vector returned from dimension probe"))
}

pub fn rerank_results(
    reranker: &mut TextRerank,
    query: &str,
    results: &[(String, String, f32)],
) -> Result<Vec<(String, String, f32)>> {
    if results.is_empty() {
        return Ok(vec![]);
    }

    let documents: Vec<&str> = results
        .iter()
        .map(|(_, snippet, _)| snippet.as_str())
        .collect();
    let reranked = reranker
        .rerank(query, &documents, false, None)
        .map_err(|e| anyhow!("Reranking failed: {}", e))?;

    Ok(reranked
        .into_iter()
        .map(|RerankResult { index, score, .. }| {
            let (path, snippet, _) = &results[index];
            (path.clone(), snippet.clone(), score)
        })
        .collect())
}
