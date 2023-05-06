use std::future::Future;

pub enum Either<L, R> {
    Left(L),
    Right(R),
}

pub async fn select2<L, R>(left: &mut L, right: &mut R) -> Either<L::Output, R::Output>
where
    L: Future + Send + Unpin,
    R: Future + Send + Unpin,
{
    tokio::select! {
        left = left => Either::Left(left),
        right = right => Either::Right(right),
    }
}
