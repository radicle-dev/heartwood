use keri_core::actor::prelude::Message;
use keri_core::actor::{event_generator, parse_event_stream};
use keri_core::database::DbError;
use keri_core::event::KeyEvent;
use keri_core::event_message::cesr_adapter::EventType;
use keri_core::event_message::msg::KeriEvent;
use keri_core::event_message::signed_event_message::{
    Notice, SignedEventMessage, SignedNontransferableReceipt, SignedTransferableReceipt,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::vec;
use std::{path::Path, sync::Arc};
//use keri_core::event_message::timestamped::Timestamped;
use keri_core::database::timestamped::Timestamped;
use keri_core::database::timestamped::TimestampedSignedEventMessage;
use keri_core::event_message::TimestampedEventMessage;
use teliox::transport::TelTransport;

//use anyhow::Result;
//use keri_controller::communication::Communication;
//use keri_controller::identifier::Identifier;
//use keri_controller::known_events::KnownEvents;
use keri_core::prefix::SelfSigningPrefix;
use keri_core::prefix::{BasicPrefix, IdentifierPrefix};
use keri_core::processor::escrow::EscrowConfig;
use keri_core::signer::CryptoBox;
use keri_core::signer::KeyManager as _;
use keri_core::{
    database::SledEventDatabase, event_message::signature::Signature,
    processor::validator::EventValidator,
};

use serde::{Deserialize, Serialize};

/*
struct RadicleEvents {
    db: Arc<SledEventDatabase>,
}

impl RadicleEvents {
    fn incept(
        &self,
        public_keys: Vec<BasicPrefix>,
        next_public_keys: Vec<BasicPrefix>,
        // TODO: Witnesses,
        // TODO: Escrow config,
    ) -> Result<String, keri_core::error::Error> {
        event_generator::incept(
            public_keys,
            next_public_keys,
            vec![],
            0,
            None,
        )
    }

        /// Adds signature to event and processes it.
    /// Should call `IdentifierController::notify_witnesses` after calling this function.
    pub fn finalize_key_event(
        &self,
        event: &KeriEvent<KeyEvent>,
        sig: &SelfSigningPrefix,
        own_index: usize,
    ) -> Result<(), MechanicsError> {
        let signature = IndexedSignature::new_both_same(sig.clone(), own_index as u16);

        let signed_message = event.sign(vec![signature], None, None);
        // self.processor.process_own_event(signed_message)?;
        self.process(&Message::Notice(Notice::Event(signed_message)))?;

        Ok(())
    }


    /// Verifies event signature and adds it to kel.
    /// Returns new established identifier prefix.
    /// Meant to be used for identifiers with one key pair.
    /// Must call `IdentifierController::notify_witnesses` after calling this function.
    pub fn finalize_inception(
        &self,
        event: &[u8],
        sig: &SelfSigningPrefix,
    ) -> Result<IdentifierPrefix, MechanicsError> {
        let parsed_event =
            parse_event_type(event).map_err(|_e| MechanicsError::EventFormatError)?;
        match parsed_event {
            EventType::KeyEvent(ke) => {
                if let EventData::Icp(_) = &ke.data.get_event_data() {
                    // TODO we assume here that provided signature matches 0th public key.
                    self.finalize_key_event(&ke, sig, 0)?;
                    Ok(ke.data.get_prefix())
                } else {
                    Err(MechanicsError::InceptionError(
                        "Wrong event type, should be inception event".into(),
                    ))
                }
            }
            _ => Err(MechanicsError::InceptionError(
                "Wrong event type, should be inception event".into(),
            )),
        }
    }

        // Returns messages if they can be returned immediately, i.e. for query message
    pub fn process(&self, msg: &Message) -> Result<Option<Vec<Message>>, Error> {
        let response = match msg.clone() {
            Message::Op(op) => todo!(),
            Message::Notice(notice) => {
                match notice {
                    Notice::Event(event) => {
                        self.db.add_kel_finalized_event(event, id)
                    }
                }
                None
            }
        };

        Ok(response)
    }
}

fn foo() -> Result<(), anyhow::Error> {
    let db_path = PathBuf::from("keri.db");
    let db = SledEventDatabase::new(db_path).unwrap();
    let x = EventValidator::new(Arc::new(db));

    let mut key_manager = CryptoBox::new().unwrap();

    // Incept identifier.
    // The `Identifier` structure facilitates the management of the
    // Key Event Log specific to a particular identifier.
    let pk = BasicPrefix::Ed25519(key_manager.public_key());
    let npk = BasicPrefix::Ed25519(key_manager.next_public_key());

    let escrow_config = EscrowConfig::default();

    // TODO: Path? EscrowConfig?
    let known_events = RadicleEvents {};

    // NOTE: We do not call the controller `incept` function here, but instead modify known events 'manually'.
    let icp_event = known_events.incept(vec![pk], vec![npk])?;

    let signature =
        SelfSigningPrefix::Ed25519Sha512(key_manager.sign(icp_event.as_bytes()).unwrap());

    let signature =
        SelfSigningPrefix::Ed25519Sha512(key_manager.sign(icp_event.as_bytes()).unwrap());

    // NOTE: `Communication` has `async` functions :(

    let transport = TestTransport::new(HashMap::new());

    let tel_transport = TelTransport {};

    let telt: Box<(dyn teliox::transport::GeneralTelTransport + Send + Sync + 'static)> =
        Box::new(tel_transport);

    let known_events_2 = Arc::new(KnownEvents::new(db_path, escrow_config)?);
    let known_events_3 = Arc::new(KnownEvents::new(db_path, escrow_config)?);

    // NOTE: We do not call the controller `finalize_inception` function here, but instead modify known events 'manually'.
    let initialized_id = known_events
        .finalize_inception(icp_event.as_bytes(), &signature)
        .unwrap();
    let signer_identifier = Identifier::new(
        initialized_id,
        None,
        known_events_2,
        Arc::new(Communication::new(
            known_events_3,
            Box::new(transport),
            telt,
        )),
    );

    println!("Signer: {}", &signing_identifier.id());

    Ok(())
}
*/

/*/
fn bar() -> Result<()> {
    let sig = Signature::new();
    let msg = "abc".as_bytes();

    let db = SledEventDatabase::new(Path::new("db")).unwrap();
    let x = EventValidator::new(Eventstorage::new(db));

    let ourdb = RadicleEventDatabase::new();
    let ourvalidator = RadicleEventValidator::new(ourdb);

    assert_eq!(x.verify(msg, sig), oourvalidator.verify(msg, sig));
    Ok(())
}
*/

trait EventDatabase {
    fn add_kel_finalized_event(
        &mut self,
        event: SignedEventMessage,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError>;

    fn get_kel_finalized_events(
        &mut self,
        id: &IdentifierPrefix,
    ) -> Option<
        impl Iterator<
            Item = keri_core::database::timestamped::Timestamped<SignedEventMessage>,
        >,
    >;

    fn remove_kel_finalized_event(
        &self,
        id: &IdentifierPrefix,
        event: &SignedEventMessage,
    ) -> Result<(), DbError>;

    fn add_receipt_t(
        &self,
        receipt: SignedTransferableReceipt,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError>;

    fn get_receipts_t(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = SignedTransferableReceipt>>;

    fn add_receipt_nt(
        &self,
        receipt: SignedNontransferableReceipt,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError>;

    fn get_receipts_nt(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = SignedNontransferableReceipt>>;

    fn remove_receipts_nt(&self, id: &IdentifierPrefix) -> Result<(), DbError>;

    fn add_likely_duplicious_event(
        &self,
        event: KeriEvent<KeyEvent>,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError>;

    fn get_likely_duplicitous_events(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = TimestampedEventMessage>>;

    fn add_duplicious_event(
        &self,
        event: SignedEventMessage,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError>;

    fn get_duplicious_events(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = Timestamped<SignedEventMessage>>>;
}

impl EventDatabase for SledEventDatabase {
    fn add_kel_finalized_event(
        &mut self,
        event: SignedEventMessage,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        Self::add_kel_finalized_event(&self, event, id)
    }

    fn get_kel_finalized_events(
        &mut self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = TimestampedSignedEventMessage>> {
        Self::get_kel_finalized_events(&self, id)
    }

    fn remove_kel_finalized_event(
        &self,
        id: &IdentifierPrefix,
        event: &SignedEventMessage,
    ) -> Result<(), DbError> {
        Self::remove_kel_finalized_event(&self, id, event)
    }

    fn add_receipt_t(
        &self,
        receipt: SignedTransferableReceipt,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        Self::add_receipt_t(&self, receipt, id)
    }

    fn get_receipts_t(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = SignedTransferableReceipt>> {
        Self::get_receipts_t(&self, id)
    }

    fn add_receipt_nt(
        &self,
        receipt: SignedNontransferableReceipt,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        Self::add_receipt_nt(&self, receipt, id)
    }

    fn get_receipts_nt(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = SignedNontransferableReceipt>> {
        Self::get_receipts_nt(&self, id)
    }

    fn remove_receipts_nt(&self, id: &IdentifierPrefix) -> Result<(), DbError> {
        Self::remove_receipts_nt(&self, id)
    }

    fn add_likely_duplicious_event(
        &self,
        event: KeriEvent<KeyEvent>,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        Self::add_likely_duplicious_event(&self, event, id)
    }

    fn get_likely_duplicitous_events(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = TimestampedEventMessage>> {
        Self::get_likely_duplicitous_events(&self, id)
    }

    fn add_duplicious_event(
        &self,
        event: SignedEventMessage,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        Self::add_duplicious_event(&self, event, id)
    }

    fn get_duplicious_events(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = Timestamped<SignedEventMessage>>> {
        Self::get_duplicious_events(&self, id)
    }
}

#[derive(Serialize, Deserialize)]
struct RadicleEventDatabase {
    kel: HashMap<IdentifierPrefix, Vec<Timestamped<SignedEventMessage>>>,
    // repo: Repository,
}

impl RadicleEventDatabase {
    pub fn new() -> Self {
        Self {
            kel: HashMap::new(),
        }
    }
}

impl EventDatabase for RadicleEventDatabase {
    fn add_kel_finalized_event(
        &mut self,
        event: SignedEventMessage,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        self.kel
            .entry(id.clone())
            .or_default()
            .push(Timestamped::new(event));
        Ok(())
    }

    fn get_kel_finalized_events(
        &mut self,
        id: &IdentifierPrefix,
    ) -> Option<
        impl Iterator<
            Item = keri_core::database::timestamped::Timestamped<SignedEventMessage>,
        >,
    > {
        Some(self.kel.entry(id.clone()).or_default().iter().cloned())
    }

    fn remove_kel_finalized_event(
        &self,
        id: &IdentifierPrefix,
        event: &SignedEventMessage,
    ) -> Result<(), DbError> {
        todo!()
    }

    fn add_receipt_t(
        &self,
        receipt: SignedTransferableReceipt,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        todo!()
    }

    fn get_receipts_t(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = SignedTransferableReceipt>> {
        Some(std::iter::empty())
    }

    fn add_receipt_nt(
        &self,
        receipt: SignedNontransferableReceipt,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        todo!()
    }

    fn get_receipts_nt(
        &self,
        id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = SignedNontransferableReceipt>> {
        Some(std::iter::empty())
    }

    fn remove_receipts_nt(&self, id: &IdentifierPrefix) -> Result<(), DbError> {
        todo!()
    }

    fn add_likely_duplicious_event(
        &self,
        event: KeriEvent<KeyEvent>,
        id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        // TODO(lorenz): Think about whether we want to implement duplicious events.
        Ok(())
    }

    fn get_likely_duplicitous_events(
        &self,
        _id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = TimestampedEventMessage>> {
        // TODO(lorenz): Think about whether we want to implement duplicious events.
        Some(std::iter::empty())
    }

    fn add_duplicious_event(
        &self,
        _event: SignedEventMessage,
        _id: &IdentifierPrefix,
    ) -> Result<(), DbError> {
        // TODO(lorenz): Think about whether we want to implement duplicious events.
        Ok(())
    }

    fn get_duplicious_events(
        &self,
        _id: &IdentifierPrefix,
    ) -> Option<impl Iterator<Item = Timestamped<SignedEventMessage>>> {
        // TODO(lorenz): Think about whether we want to implement duplicious events.
        Some(std::iter::empty())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn first_test() -> Result<(), anyhow::Error> {
        let dir = tempdir()?;
        let temp = dir.path().join("db");

        let prefix: IdentifierPrefix = "ENRc2DeK48BKJ3ST8mypvngVwEAxw9rZr_GPNP25TmQ_"
            .parse()
            .unwrap();

        let db_sled = SledEventDatabase::new(&temp).unwrap();
        let mut db_rad = RadicleEventDatabase::new();

        let events_raw = r#"{"v":"KERI10JSON000159_","t":"icp","d":"ENRc2DeK48BKJ3ST8mypvngVwEAxw9rZr_GPNP25TmQ_","i":"ENRc2DeK48BKJ3ST8mypvngVwEAxw9rZr_GPNP25TmQ_","s":"0","kt":"1","k":["DDhKvndcqZlJNx-mtC_5eTy7UiuxOPVgAV3HsmofP2Ll"],"nt":"1","n":["EOOlrw-1jQHp8IfE1mfOb_ikXpHksVSyZ0RCnu5X0Rfg"],"bt":"0","b":["DNjeO6mfXSbrFFdk5UjDmioaho6ON0Sp6JMfhKz2jJF-"],"c":[],"a":[]}-AABAAD34oIVuxmLLKldRCzhfxR9hNg2SOMbOKZn1hp6D1OBmA6Hut6gSID21vFk50ost8_-VNbjHIQnHZ8WulUJ_yQL{"v":"KERI10JSON000091_","t":"rct","d":"ENRc2DeK48BKJ3ST8mypvngVwEAxw9rZr_GPNP25TmQ_","i":"ENRc2DeK48BKJ3ST8mypvngVwEAxw9rZr_GPNP25TmQ_","s":"0"}-CABDNjeO6mfXSbrFFdk5UjDmioaho6ON0Sp6JMfhKz2jJF-0BCTI8wy4v_Iyiyh4cSuG02R-K4GZHMXQVyC3yckpPpev0yWUiuera6q7ErDIQDLvjnMG5UuE2Ycw8-sxzeybicP"#;
        let to_process = parse_event_stream(events_raw.as_bytes())?;

        for msg in to_process {
            match msg {
                Message::Notice(Notice::Event(event)) => {
                    db_sled.add_kel_finalized_event(event.clone(), &prefix)?;
                    db_rad.add_kel_finalized_event(event, &prefix)?;
                }
                _ => {
                    // Ignore all other messages, they are not relevant for this test.
                },
            }
        }

        let result_sled = db_sled
                .get_kel_finalized_events(&prefix)
                .unwrap()
                .collect::<Vec<_>>();

        let result_rad = db_rad
                .get_kel_finalized_events(&prefix)
                .unwrap()
                .collect::<Vec<_>>();

        assert_eq!(result_sled.len(), result_rad.len());
        assert_eq!(result_sled, result_rad);

        Ok(())
    }
}
