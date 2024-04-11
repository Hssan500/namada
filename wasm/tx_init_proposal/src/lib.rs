//! A tx to create a governance proposal.

use namada_tx_prelude::action::{Action, GovAction, Write};
use namada_tx_prelude::*;

#[transaction]
fn apply_tx(ctx: &mut Ctx, tx: Tx) -> TxResult {
    let data = tx.data().ok_or_err_msg("Missing data").map_err(|err| {
        ctx.set_commitment_sentinel();
        err
    })?;
    let tx_data = governance::InitProposalData::try_from_slice(&data[..])
        .wrap_err("Failed to decode InitProposalData value")?;

    // The tx must be authorized by the author address
    ctx.insert_verifier(&tx_data.author)?;

    ctx.push_action(Action::Gov(GovAction::InitProposal {
        author: tx_data.author.clone(),
    }))?;

    // Get the content from the referred to section
    let content = tx
        .get_section(&tx_data.content)
        .ok_or_err_msg("Missing proposal content")
        .map_err(|err| {
            ctx.set_commitment_sentinel();
            err
        })?
        .extra_data()
        .ok_or_err_msg("Missing full proposal content")
        .map_err(|err| {
            ctx.set_commitment_sentinel();
            err
        })?;

    // Get the code from the referred to section
    let code = tx_data
        .get_section_code_hash()
        .map(|hash| {
            tx.get_section(&hash)
                .ok_or_err_msg("Missing proposal code")
                .map_err(|err| {
                    ctx.set_commitment_sentinel();
                    err
                })?
                .extra_data()
                .ok_or_err_msg("Missing full proposal code")
                .map_err(|err| {
                    ctx.set_commitment_sentinel();
                    err
                })
        })
        .transpose()
        .wrap_err("Failed to retrieve proposal code")?;

    log_string("apply_tx called to create a new governance proposal");

    governance::init_proposal(ctx, tx_data, content, code)
        .wrap_err("Failed to initialize new governance proposal")
}
