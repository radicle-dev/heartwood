use std::fmt;

use crate::{Constraint, Line};

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

#[derive(Clone, Copy)]
pub struct Context {
    pub ansi: bool,
    pub constraint: Constraint,
}

impl Default for Context {
    fn default() -> Self {
        Context {
            ansi: super::Paint::is_enabled(),
            constraint: Constraint::default(),
        }
    }
}

#[deprecated]
pub fn display<'a, T: Display<Context> + Sized + 'a>(display: &'a T) -> impl fmt::Display + 'a {
    DisplayWrapper {
        ctx: &Context {
            ansi: true,
            constraint: Constraint::UNBOUNDED,
        },
        parent: display,
    }
}
