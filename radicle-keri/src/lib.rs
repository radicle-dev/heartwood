use keri_core::{database::SledEventDatabase, event_message::signature::Signature, processor::validator::EventValidator};

mod test;

fn foo() {
	let db = SledEventDatabase::new(Path::new("db")).unwrap();
	let x = EventValidator::new(Eventstorage::new(db));
	let sig = Signature::new();
	x.verify("abc".as_bytes(), sig)
}

fn bar() {
	let sig = Signature::new();
	let msg = "abc".as_bytes();

	let db = SledEventDatabase::new(Path::new("db")).unwrap();
	let x = EventValidator::new(Eventstorage::new(db));

 	let ourdb = RadicleEventDatabase::new();
	let ourvalidator = RadicleEventValidator::new(ourdb);

	assert_eq!(x.verify(msg, sig), oourvalidator.verify(msg, sig));
}