pub trait Outcome: Send + Sync + 'static {
    #[inline]
    fn as_error(&self) -> Option<String> {
        None
    }

    #[inline]
    fn boxed(self) -> Box<dyn Outcome>
    where
        Self: Sized,
    {
        Box::new(self) as _
    }
}

impl Outcome for () {}

impl<E> Outcome for Result<(), E>
where
    E: Send + Sync + 'static + std::fmt::Display + std::fmt::Debug,
{
    fn as_error(&self) -> Option<String> {
        match self {
            Ok(..) => None,
            Err(err) => Some(err.to_string()),
        }
    }
}
