use std::sync::Arc;

use keri_controller::{
    config::ControllerConfig, controller::Controller, error::ControllerError,
    identifier::query::QueryResponse, BasicPrefix, CryptoBox, EndRole, IdentifierPrefix,
    KeyManager, LocationScheme, Oobi, SelfSigningPrefix,
};
use keri_core::processor::validator::{MoreInfoError, VerificationError};
use tempfile::Builder;

#[async_std::test]
async fn indirect_mode_signing() -> Result<(), ControllerError> {
    let first_witness_id: BasicPrefix = "BDg3H7Sr-eES0XWXiO8nvMxW6mD_1LxLeE1nuiZxhGp4"
        .parse()
        .unwrap();
    // OOBI (Out-Of-Band Introduction) specifies the way how actors can be found.
    let first_witness_oobi: LocationScheme = serde_json::from_str(&format!(
        r#"{{"eid":{:?},"scheme":"http","url":"http://witness2.sandbox.argo.colossi.network/"}}"#,
        first_witness_id
    ))
    .unwrap();

    let second_witness_id: BasicPrefix = "BDg1zxxf8u4Hx5IPraZzmStfSCZFZbDzMHjqVcFW5OfP"
        .parse()
        .unwrap();
    let second_witness_oobi: LocationScheme = serde_json::from_str(&format!(
        r#"{{"eid":{:?},"scheme":"http","url":"http://witness3.sandbox.argo.colossi.network/"}}"#,
        second_witness_id
    ))
    .unwrap();

    // Setup database path and key manager.
    let database_path = Builder::new().prefix("test-db0").tempdir().unwrap();
    let mut key_manager = CryptoBox::new().unwrap();

    // The `Controller` structure aggregates all known KEL events (across all
    // identifiers) and offers functions for retrieving them, verifying the
    // integrity of new events, and conducting signature verification.
    let signer_controller = Arc::new(Controller::new(ControllerConfig {
        db_path: database_path.path().to_owned(),
        ..Default::default()
    })?);

    // Incept identifier.
    // The `Identifier` structure facilitates the management of the
    // Key Event Log specific to a particular identifier.
    let pk = BasicPrefix::Ed25519(key_manager.public_key());
    let npk = BasicPrefix::Ed25519(key_manager.next_public_key());

    // Create inception event, that needs one witness receipt to be accepted.
    let icp_event = signer_controller
        .incept(
            vec![pk],
            vec![npk],
            vec![first_witness_oobi.clone(), second_witness_oobi.clone()],
            1,
        )
        .await?;
    let signature =
        SelfSigningPrefix::Ed25519Sha512(key_manager.sign(icp_event.as_bytes()).unwrap());

    let mut signing_identifier =
        signer_controller.finalize_incept(icp_event.as_bytes(), &signature)?;

    println!("Signer: {}", &signing_identifier.id());

    // The Event Seal specifies the stage of KEL at the time of signature creation.
    // This enables us to retrieve the correct public keys from KEL during verification.
    // Trying to get current actor event seal.
    let inception_event_seal = signing_identifier.get_last_establishment_event_seal();

    // It fails because witness receipts are missing.
    assert!(matches!(
        inception_event_seal,
        Err(ControllerError::UnknownIdentifierError)
    ));

    // Publish event to actor's witnesses
    signing_identifier.notify_witnesses().await.unwrap();

    // Querying witness to get receipts
    for qry in signing_identifier
        .query_mailbox(signing_identifier.id(), &[first_witness_id.clone()])
        .unwrap()
    {
        let signature =
            SelfSigningPrefix::Ed25519Sha512(key_manager.sign(&qry.encode().unwrap()).unwrap());
        signing_identifier
            .finalize_query_mailbox(vec![(qry, signature)])
            .await
            .unwrap();
    }

    // Now KEL event should be accepted and event seal exists.
    let signing_event_seal = signing_identifier.get_last_establishment_event_seal();
    assert!(signing_event_seal.is_ok());
    let signing_event_seal = signing_event_seal.unwrap();

    // Sign message with established identifier
    let first_message = "Hi".as_bytes();
    let first_message_signature = vec![SelfSigningPrefix::Ed25519Sha512(
        key_manager.sign(first_message).unwrap(),
    )];

    let first_signature = signing_identifier.sign_data(first_message, &first_message_signature)?;

    // Establish verifying identifier
    // Setup database path and key manager.
    let verifier_database_path = Builder::new().prefix("test-db1").tempdir().unwrap();
    let verifier_key_manager = CryptoBox::new().unwrap();

    let verifying_controller = Arc::new(Controller::new(ControllerConfig {
        db_path: verifier_database_path.path().to_owned(),
        ..Default::default()
    })?);

    let pk = BasicPrefix::Ed25519(verifier_key_manager.public_key());
    let npk = BasicPrefix::Ed25519(verifier_key_manager.next_public_key());

    // Create inception event, that needs one witness receipt to be accepted.
    let icp_event = verifying_controller
        .incept(
            vec![pk],
            vec![npk],
            vec![first_witness_oobi.clone(), second_witness_oobi.clone()],
            1,
        )
        .await?;
    let signature =
        SelfSigningPrefix::Ed25519Sha512(verifier_key_manager.sign(icp_event.as_bytes()).unwrap());

    let mut verifying_identifier =
        verifying_controller.finalize_incept(icp_event.as_bytes(), &signature)?;

    println!("Verifier: {}", verifying_identifier.id());

    // Publish event to actor's witnesses
    verifying_identifier.notify_witnesses().await.unwrap();

    // Querying witness to get receipts
    for qry in verifying_identifier
        .query_mailbox(verifying_identifier.id(), &[first_witness_id.clone()])
        .unwrap()
    {
        let signature = SelfSigningPrefix::Ed25519Sha512(
            verifier_key_manager.sign(&qry.encode().unwrap()).unwrap(),
        );
        verifying_identifier
            .finalize_query_mailbox(vec![(qry, signature)])
            .await
            .unwrap();
    }

    // Check if verifying identifier was established successfully
    let inception_event_seal = verifying_identifier.get_last_establishment_event_seal();
    assert!(inception_event_seal.is_ok());

    // Now setup watcher, to be able to query of signing identifier KEL.
    let watcher_id: IdentifierPrefix = "BF2t2NPc1bwptY1hYV0YCib1JjQ11k9jtuaZemecPF5b"
        .parse()
        .unwrap();
    let watcher_oobi: Oobi = serde_json::from_str(&format!(
        r#"{{"eid":"{}","scheme":"http","url":"http://watcher.sandbox.argo.colossi.network/"}}"#,
        watcher_id
    ))
    .unwrap();

    // Resolve watcher oobi
    verifying_identifier.resolve_oobi(&watcher_oobi).await?;

    // Generate and sign event, that will be sent to watcher, so it knows to act
    // as verifier's watcher.
    let add_watcher = verifying_identifier.add_watcher(watcher_id)?;
    let signature = SelfSigningPrefix::Ed25519Sha512(
        verifier_key_manager.sign(add_watcher.as_bytes()).unwrap(),
    );

    verifying_identifier
        .finalize_add_watcher(add_watcher.as_bytes(), signature)
        .await?;

    // Now query about signer's kel.
    // To find `signing_identifier`s KEL, `verifying_identifier` needs to
    // provide to watcher its oobi and oobi of its witnesses.
    for wit_oobi in vec![first_witness_oobi, second_witness_oobi] {
        let oobi = Oobi::Location(wit_oobi);
        verifying_identifier.resolve_oobi(&oobi).await?;
        verifying_identifier
            .send_oobi_to_watcher(&verifying_identifier.id(), &oobi)
            .await?;
    }
    let signer_oobi = EndRole {
        cid: signing_identifier.id().clone(),
        role: keri_core::oobi::Role::Witness,
        eid: keri_controller::IdentifierPrefix::Basic(second_witness_id.clone()),
    };

    verifying_identifier
        .send_oobi_to_watcher(&verifying_identifier.id(), &Oobi::EndRole(signer_oobi))
        .await?;

    // Query kel of signing identifier
    let queries_and_signatures: Vec<_> = verifying_identifier
        .query_watchers(&signing_event_seal)?
        .into_iter()
        .map(|qry| {
            let signature = SelfSigningPrefix::Ed25519Sha512(
                verifier_key_manager.sign(&qry.encode().unwrap()).unwrap(),
            );
            (qry, signature)
        })
        .collect();

    let (mut response, _) = verifying_identifier
        .finalize_query(queries_and_signatures.clone())
        .await;
    // Watcher might need some time to find KEL. Ask about it until it's ready.
    while let QueryResponse::NoUpdates = response {
        (response, _) = verifying_identifier
            .finalize_query(queries_and_signatures.clone())
            .await;
    }

    // Verify signed message.
    assert!(verifying_controller
        .verify(first_message, &first_signature)
        .is_ok());

    // Rotate signer keys
    key_manager.rotate()?;
    let pk = BasicPrefix::Ed25519(key_manager.public_key());
    let npk = BasicPrefix::Ed25519(key_manager.next_public_key());

    // Rotation needs two witness receipts to be accepted
    let rotation_event = signing_identifier
        .rotate(vec![pk], vec![npk], 1, vec![], vec![], 2)
        .await?;

    let signature = SelfSigningPrefix::Ed25519Sha512(key_manager.sign(rotation_event.as_bytes())?);
    signing_identifier
        .finalize_rotate(rotation_event.as_bytes(), signature)
        .await?;

    // Publish event to actor's witnesses
    signing_identifier.notify_witnesses().await.unwrap();

    // Querying witnesses to get receipts
    for qry in signing_identifier
        .query_mailbox(
            signing_identifier.id(),
            &[first_witness_id.clone(), second_witness_id.clone()],
        )
        .unwrap()
    {
        let signature =
            SelfSigningPrefix::Ed25519Sha512(key_manager.sign(&qry.encode().unwrap()).unwrap());
        signing_identifier
            .finalize_query_mailbox(vec![(qry, signature)])
            .await
            .unwrap();
    }

    // Sign message with rotated keys.
    let second_message = "Hi".as_bytes();
    let second_message_signature = vec![SelfSigningPrefix::Ed25519Sha512(
        key_manager.sign(second_message).unwrap(),
    )];

    let current_event_seal = signing_identifier.get_last_establishment_event_seal()?;
    let second_signature =
        signing_identifier.sign_data(second_message, &second_message_signature)?;

    // Try to verify it, it should fail, because verifier doesn't know signer's rotation event.
    assert!(matches!(
        verifying_controller
            .verify(second_message, &second_signature)
            .unwrap_err(),
        VerificationError::MoreInfo(MoreInfoError::EventNotFound(_))
    ));

    // Query kel of signing identifier
    let queries_and_signatures: Vec<_> = verifying_identifier
        .query_watchers(&current_event_seal)?
        .into_iter()
        .map(|qry| {
            let signature = SelfSigningPrefix::Ed25519Sha512(
                verifier_key_manager.sign(&qry.encode().unwrap()).unwrap(),
            );
            (qry, signature)
        })
        .collect();

    let (mut response, _) = verifying_identifier
        .finalize_query(queries_and_signatures.clone())
        .await;
    // Watcher might need some time to find KEL. Ask about it until it's ready.
    while let QueryResponse::NoUpdates = response {
        (response, _) = verifying_identifier
            .finalize_query(queries_and_signatures.clone())
            .await;
    }

    let verification_result = verifying_controller.verify(second_message, &second_signature);
    assert!(verification_result.is_ok());

    Ok(())
}
