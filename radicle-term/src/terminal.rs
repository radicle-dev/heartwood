use std::{env, fmt, io, process};
use crate::format;
use crate::{display, style, Paint, Size, Display, Context};

use crate::{ERROR_PREFIX, ERROR_HINT_PREFIX, TAB, WARNING_PREFIX};

trait Terminal {
type Context;

fn success_args<W: io::Write>(&self, w: &mut W, args: fmt::Arguments);

fn tip_args(&self, args: fmt::Arguments);

fn notice_args<W: io::Write>(&self, w: &mut W, args: fmt::Arguments);

fn columns(&self, ) -> Option<usize>;

fn rows(&self, ) -> Option<usize> ;

fn viewport(&self, ) -> Option<Size>; 

fn headline(&self, headline: impl fmt::Display);

fn header(&self, header: &str);

fn blob(&self, text: impl fmt::Display);

fn blank(&self, );

fn print<'a>(&self, msg: &'a impl Display<'a, Self::Context>);

fn prefixed(&self, prefix: &str, text: &str) -> String;

fn help(&self, name: &str, version: &str, description: &str, usage: &str);

fn usage(&self, name: &str, usage: &str, context: &Self::Context);

fn println(&self, prefix: impl fmt::Display, msg: impl fmt::Display);

fn indented(&self, msg: impl fmt::Display);

fn indented_display<'a>(&self, msg: &'a impl Display<'a>);

/*
fn subcommand(&self, msg: impl fmt::Display);
*/

fn warning(&self, warning: impl fmt::Display);

fn error(&self, error: impl fmt::Display);

fn hint(&self, hint: impl fmt::Display);

fn ask<D: fmt::Display>(&self, prompt: D, default: bool) -> bool;

fn confirm<D: fmt::Display>(&self, prompt: D) -> bool;

fn abort<D: fmt::Display>(&self, prompt: D) -> bool;

fn input<S, E>(&self, message: &str, default: Option<S>, help: Option<&str>) -> anyhow::Result<S>
where
    S: fmt::Display + std::str::FromStr<Err = E> + Clone,
    E: fmt::Debug + fmt::Display,
;

fn markdown(&self, content: &str);

}

struct ContextTerminal {
	ctx: Context,
}

impl Terminal for ContextTerminal {
	type Context = Context;
	
	fn success_args<W: io::Write>(&self, w: &mut W, args: fmt::Arguments) {
		todo!()
	    }
	
	fn tip_args(&self, args: fmt::Arguments) {
		todo!()
	    }
	
	fn notice_args<W: io::Write>(&self, w: &mut W, args: fmt::Arguments) {
		todo!()
	    }
	
	fn columns(&self, ) -> Option<usize> {
		todo!()
	    }
	
	fn rows(&self, ) -> Option<usize>  {
		todo!()
	    }
	
	fn viewport(&self, ) -> Option<Size> {
		todo!()
	    }
	
	fn headline(&self, headline: impl fmt::Display) {
		todo!()
	    }
	
	fn header(&self, header: &str) {
		todo!()
	    }
	
	fn blob(&self, text: impl fmt::Display) {
		todo!()
	    }
	
	fn blank(&self, ) {
		todo!()
	    }
	
	fn print<'a>(&self, msg: &'a impl Display<'a, Self::Context>) {
		// println!("{}", msg.display(&self.ctx));
		msg.fmt_with(f, &ctx)
	    }
	
	fn prefixed(&self, prefix: &str, text: &str) -> String {
		todo!()
	    }
	
	fn help(&self, name: &str, version: &str, description: &str, usage: &str) {
		todo!()
	    }
	
	fn usage(&self, name: &str, usage: &str, context: &Self::Context) {
		todo!()
	    }
	
	fn println(&self, prefix: impl fmt::Display, msg: impl fmt::Display) {
		todo!()
	    }
	
	fn indented(&self, msg: impl fmt::Display) {
		todo!()
	    }
	
	fn indented_display<'a>(&self, msg: &'a impl Display<'a>) {
		todo!()
	    }
	
	fn warning(&self, warning: impl fmt::Display) {
		todo!()
	    }
	
	fn error(&self, error: impl fmt::Display) {
		todo!()
	    }
	
	fn hint(&self, hint: impl fmt::Display) {
		todo!()
	    }
	
	fn ask<D: fmt::Display>(&self, prompt: D, default: bool) -> bool {
		todo!()
	    }
	
	fn confirm<D: fmt::Display>(&self, prompt: D) -> bool {
		todo!()
	    }
	
	fn abort<D: fmt::Display>(&self, prompt: D) -> bool {
		todo!()
	    }
	
	fn input<S, E>(&self, message: &str, default: Option<S>, help: Option<&str>) -> anyhow::Result<S>
	    where
		S: fmt::Display + std::str::FromStr<Err = E> + Clone,
		E: fmt::Debug + fmt::Display,
	     {
		todo!()
	    }
	
	fn markdown(&self, content: &str) {
		todo!()
	    }
}