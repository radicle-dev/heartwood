use std::fmt;

pub trait Display<C = Context> {
    fn fmt_with<'a>(&'a self, f: &mut fmt::Formatter<'_>, ctx: &'a C) -> fmt::Result;
}

pub(crate) struct DisplayWrapper<'a, T: Display<C>, C> {
    ctx: &'a C,
    parent: &'a T,
}

impl<'a, T: Display<C>, C> DisplayWrapper<'a, T, C> {
    pub fn new(parent: &'a T, ctx: &'a C) -> Self {
        Self { ctx, parent }
    }
}

impl<'a, T: Display<C>, C> fmt::Display for DisplayWrapper<'a, T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.parent.fmt_with(f, self.ctx)
    }
}

impl<T: fmt::Display, C> Display<C> for T {
    fn fmt_with(&self, f: &mut fmt::Formatter<'_>, _: &C) -> fmt::Result {
        self.fmt(f)
    }
}

pub struct Context {
    pub ansi: bool,
}

impl Default for Context {
    fn default() -> Self {
        Context {
            ansi: super::Paint::is_enabled(),
        }
    }
}

#[deprecated]
pub fn display_with<'a, T: Display<C>, C>(display: &'a T, ctx: &'a C) -> impl fmt::Display + 'a {
    DisplayWrapper {
        ctx,
        parent: display,
    }
}

#[deprecated]
pub fn display<'a, T: Display<Context> + Sized + 'a>(display: &'a T) -> impl fmt::Display + 'a {
    DisplayWrapper {
        ctx: &Context { ansi: true },
        parent: display,
    }
}
