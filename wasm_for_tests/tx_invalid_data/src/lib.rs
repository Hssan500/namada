use namada_tx_prelude::*;

#[transaction]
fn apply_tx(ctx: &mut Ctx, tx_data: BatchedTx) -> TxResult {
    let BatchedTx {
        tx: signed,
        ref cmt,
    } = tx_data;
    let _data =
        signed
            .data(cmt)
            .ok_or_err_msg("Missing data")
            .map_err(|err| {
                ctx.set_commitment_sentinel();
                err
            })?;
    Ok(())
}
