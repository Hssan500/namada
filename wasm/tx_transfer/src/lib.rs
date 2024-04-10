//! A tx for token transfer.
//! This tx uses `token::Transfer` wrapped inside `SignedTxData`
//! as its input as declared in `namada` crate.

use namada_tx_prelude::*;

#[transaction]
fn apply_tx(ctx: &mut Ctx, tx_data: Tx) -> TxResult {
    let signed = tx_data;
    let data = signed.data().ok_or_err_msg("Missing data").map_err(|err| {
        ctx.set_commitment_sentinel();
        err
    })?;
    let transfer = token::Transfer::try_from_slice(&data[..])
        .wrap_err("Failed to decode token::Transfer tx data")?;
    debug_log!("apply_tx called with transfer: {:#?}", transfer);

    token::transfer(
        ctx,
        &transfer.source,
        &transfer.target,
        &transfer.token,
        transfer.amount.amount(),
    )
    .wrap_err("Token transfer failed")?;

    let shielded = transfer
        .shielded
        .as_ref()
        .map(|hash| {
            signed
                .get_section(hash)
                .and_then(|x| x.as_ref().masp_tx())
                .ok_or_err_msg(
                    "Unable to find required shielded section in tx data",
                )
                .map_err(|err| {
                    ctx.set_commitment_sentinel();
                    err
                })
        })
        .transpose()?;
    if let Some(shielded) = shielded {
        token::utils::handle_masp_tx(ctx, &shielded, transfer.key.as_deref())
            .wrap_err("Encountered error while handling MASP transaction")?;
        update_masp_note_commitment_tree(&shielded)
            .wrap_err("Failed to update the MASP commitment tree")?;
    }
    Ok(())
}
