use std::fmt;

use super::Paint;

pub trait Display<'a, C=Context>
where Self: Sized
{
    fn display(&'a self, ctx: &'a C) -> impl fmt::Display + 'a {
        DisplayWrapper {
            ctx,
            parent: self,
        }
    }

    fn fmt_with(&'a self, f: &mut fmt::Formatter<'_>, ctx: &'a C) -> fmt::Result;
}

struct DisplayWrapper<'a, T: Display<'a, C>, C> {
    ctx: &'a C,
    parent: &'a T,
}

impl<'a, T: Display<'a, C>, C> fmt::Display for DisplayWrapper<'a, T, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
	    self.parent.fmt_with(f, &self.ctx)
    }
}

impl<'a, T: fmt::Display, C> Display<'a, C> for T {
    fn fmt_with(&self, f: &mut fmt::Formatter<'_>, _: &C) -> fmt::Result {
	    self.fmt(f)
    }
}

pub struct Context {
    pub ansi: bool,
}

impl Default for Context {
    fn default() -> Self {
        Context { ansi: Paint::is_enabled() }
    }
}

pub fn display<'a, T: Display<'a, Context> + Sized + 'a>(display: &'a T) -> impl fmt::Display + 'a {
    display.display(&Context {
        ansi: true,
    })
}