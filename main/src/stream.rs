use futures_util::FutureExt;
use futures_util::Stream;
use futures_util::StreamExt;
use futures_util::TryFuture;
use futures_util::TryFutureExt;
use futures_util::TryStream;
use futures_util::TryStreamExt;

pub fn join<RE, F, S, I, FE, SE, FO>(future: F, stream: S) -> impl Stream<Item = Result<I, RE>>
where
    F: TryFuture<Ok = FO, Error = FE> + Send + 'static,
    S: TryStream<Ok = I, Error = SE> + Send + 'static,
    FE: Into<RE>,
    SE: Into<RE>,
{
    let wrapped_future = future.map_ok(|_| None).map_err(Into::into).into_stream();
    let wrapped_stream = stream.map_ok(Some).map_err(Into::into);
    futures_util::stream::select(wrapped_stream, wrapped_future)
        .filter_map(|item| async { item.transpose() })
}
