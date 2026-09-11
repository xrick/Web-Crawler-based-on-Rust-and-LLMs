//! Injectable I/O boundaries keep orchestration tests independent of external services.
use crate::{
    llm,
    models::{Block, Specification},
    network::Downloader,
};
use std::{future::Future, path::Path, pin::Pin};
use tokio_util::sync::CancellationToken;

pub type IoFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + 'a>>;
pub trait Fetcher {
    fn robots<'a>(&'a mut self, url: &'a str) -> IoFuture<'a, String>;
    fn get<'a>(&'a self, url: &'a str) -> IoFuture<'a, (String, String)>;
}
pub trait DownloadFactory: Send + Sync {
    fn create(&self, cancel: CancellationToken) -> Result<Box<dyn Fetcher>, String>;
}
pub struct AppleDownloads;
impl DownloadFactory for AppleDownloads {
    fn create(&self, cancel: CancellationToken) -> Result<Box<dyn Fetcher>, String> {
        Ok(Box::new(Downloader::new(cancel)?))
    }
}
impl Fetcher for Downloader {
    fn robots<'a>(&'a mut self, url: &'a str) -> IoFuture<'a, String> {
        Box::pin(async move {
            let (_, body) = self.get(url).await?;
            self.robots = crate::network::Robots::parse(&body);
            Ok(body)
        })
    }
    fn get<'a>(&'a self, url: &'a str) -> IoFuture<'a, (String, String)> {
        Box::pin(Downloader::get(self, url))
    }
}
pub trait Annotator: Send + Sync {
    fn models(&self) -> IoFuture<'_, Vec<String>>;
    fn annotate<'a>(
        &'a self,
        model: &'a str,
        name: &'a str,
        blocks: &'a [Block],
        cancel: &'a CancellationToken,
        dir: &'a Path,
        index: usize,
    ) -> IoFuture<'a, (Vec<Specification>, usize, u128)>;
}
pub struct Ollama;
impl Annotator for Ollama {
    fn models(&self) -> IoFuture<'_, Vec<String>> {
        Box::pin(llm::models())
    }
    fn annotate<'a>(
        &'a self,
        model: &'a str,
        name: &'a str,
        blocks: &'a [Block],
        cancel: &'a CancellationToken,
        dir: &'a Path,
        index: usize,
    ) -> IoFuture<'a, (Vec<Specification>, usize, u128)> {
        Box::pin(llm::annotate(model, name, blocks, cancel, dir, index))
    }
}
