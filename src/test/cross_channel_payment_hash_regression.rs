// Regression test for the `{payment_hash}_pending` RGB seed bug in
// `lightning::rgb_utils::color_commitment`.
//
// Before the fix, the first channel on a node to materialize a payment hash in
// a given direction consumed and removed the global `{payment_hash}_pending`
// key. Any later channel on the same node processing the same payment hash in
// the same direction found neither the pending seed nor its own channel-scoped
// record, fell into the synthesis branch, and wrote an `RgbPaymentInfo` with
// `swap_payment: true` into the channel-scoped keys AND into the permanent
// `{payment_hash}` record — clobbering the originally staged metadata and
// silently disabling RGB route rewriting / retry logic for that hash.
//
// An atomic asset-for-asset swap across a shared middle hop is the natural
// shape that forces this: node 2 sees the same payment_hash on two inbound
// channels (one per asset) and two outbound channels (one per asset), so on
// each direction the second channel takes the previously-buggy fallback path.
// With the fix in place, the fallback now prefers the writer-owned permanent
// record before synthesizing, and the synthesis branch no longer overwrites
// that record, so the swap settles with consistent RGB accounting across all
// four channels on node 2.

use crate::core_types::HTLC_MIN_MSAT;

use super::*;

const TEST_DIR_BASE: &str = "tmp/cross_channel_payment_hash_regression/";

#[serial_test::serial]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[traced_test]
async fn cross_channel_payment_hash_regression() {
    initialize();

    let test_dir_node1 = format!("{TEST_DIR_BASE}node1");
    let test_dir_node2 = format!("{TEST_DIR_BASE}node2");
    let test_dir_node3 = format!("{TEST_DIR_BASE}node3");
    let (node1_addr, _) = start_node(&test_dir_node1, NODE1_PEER_PORT, false).await;
    let (node2_addr, _) = start_node(&test_dir_node2, NODE2_PEER_PORT, false).await;
    let (node3_addr, _) = start_node(&test_dir_node3, NODE3_PEER_PORT, false).await;

    fund_and_create_utxos(node1_addr, None).await;
    fund_and_create_utxos(node2_addr, None).await;
    fund_and_create_utxos(node3_addr, None).await;

    let asset_id_1 = issue_asset_nia(node1_addr).await.asset_id;
    let asset_id_2 = issue_asset_nia(node3_addr).await.asset_id;

    // Seed node 2 with both assets so it can open channels for each.
    let recipient_id = rgb_invoice(node2_addr, None, false).await.recipient_id;
    send_asset(
        node1_addr,
        &asset_id_1,
        Assignment::Fungible(400),
        recipient_id,
        None,
    )
    .await;
    mine(false);
    refresh_transfers(node2_addr).await;
    refresh_transfers(node2_addr).await;
    refresh_transfers(node1_addr).await;
    assert_eq!(asset_balance_spendable(node1_addr, &asset_id_1).await, 600);

    let recipient_id = rgb_invoice(node2_addr, None, false).await.recipient_id;
    send_asset(
        node3_addr,
        &asset_id_2,
        Assignment::Fungible(400),
        recipient_id,
        None,
    )
    .await;
    mine(false);
    refresh_transfers(node3_addr).await;
    refresh_transfers(node3_addr).await;
    refresh_transfers(node2_addr).await;
    assert_eq!(asset_balance_spendable(node3_addr, &asset_id_2).await, 600);

    let node1_pubkey = node_info(node1_addr).await.pubkey;
    let node2_pubkey = node_info(node2_addr).await.pubkey;
    let node3_pubkey = node_info(node3_addr).await.pubkey;

    // Topology that forces same-direction payment_hash collisions on node 2:
    //   chan_12 (asset1): node1 -> node2    (inbound on node2)
    //   chan_23 (asset1): node2 -> node3    (outbound on node2)
    //   chan_32 (asset2): node3 -> node2    (inbound on node2)
    //   chan_21 (asset2): node2 -> node1    (outbound on node2)
    let channel_12 = open_channel(
        node1_addr,
        &node2_pubkey,
        Some(NODE2_PEER_PORT),
        Some(50000),
        None,
        Some(500),
        Some(&asset_id_1),
    )
    .await;
    let channel_23 = open_channel(
        node2_addr,
        &node3_pubkey,
        Some(NODE3_PEER_PORT),
        Some(50000),
        None,
        Some(300),
        Some(&asset_id_1),
    )
    .await;
    let channel_32 = open_channel(
        node3_addr,
        &node2_pubkey,
        Some(NODE2_PEER_PORT),
        Some(50000),
        None,
        Some(500),
        Some(&asset_id_2),
    )
    .await;
    let channel_21 = open_channel(
        node2_addr,
        &node1_pubkey,
        Some(NODE1_PEER_PORT),
        Some(50000),
        None,
        Some(300),
        Some(&asset_id_2),
    )
    .await;

    let channels_1_before = list_channels(node1_addr).await;
    let channels_2_before = list_channels(node2_addr).await;
    let channels_3_before = list_channels(node3_addr).await;
    let chan_1_12_before = channels_1_before
        .iter()
        .find(|c| c.channel_id == channel_12.channel_id)
        .unwrap();
    let chan_1_21_before = channels_1_before
        .iter()
        .find(|c| c.channel_id == channel_21.channel_id)
        .unwrap();
    let chan_2_12_before = channels_2_before
        .iter()
        .find(|c| c.channel_id == channel_12.channel_id)
        .unwrap();
    let chan_2_23_before = channels_2_before
        .iter()
        .find(|c| c.channel_id == channel_23.channel_id)
        .unwrap();
    let chan_2_32_before = channels_2_before
        .iter()
        .find(|c| c.channel_id == channel_32.channel_id)
        .unwrap();
    let chan_2_21_before = channels_2_before
        .iter()
        .find(|c| c.channel_id == channel_21.channel_id)
        .unwrap();
    let chan_3_23_before = channels_3_before
        .iter()
        .find(|c| c.channel_id == channel_23.channel_id)
        .unwrap();
    let chan_3_32_before = channels_3_before
        .iter()
        .find(|c| c.channel_id == channel_32.channel_id)
        .unwrap();

    println!("\nsetup swap (asset1 <-> asset2, single payment_hash across both legs)");
    let maker_addr = node1_addr;
    let taker_addr = node3_addr;
    let qty_from = 20;
    let qty_to = 10;
    let maker_init_response = maker_init(
        maker_addr,
        qty_from,
        Some(&asset_id_2),
        qty_to,
        Some(&asset_id_1),
        500,
    )
    .await;
    taker(taker_addr, maker_init_response.swapstring.clone()).await;

    // Both legs of the swap share maker_init_response.payment_hash. This is the
    // key point: on node 2 the same hash will be materialized on chan_12+chan_32
    // (inbound) and on chan_23+chan_21 (outbound), exercising the fallback path
    // that the regression fix restored.
    let swaps_maker = list_swaps(maker_addr).await;
    assert_eq!(swaps_maker.maker.len(), 1);
    assert_eq!(
        swaps_maker.maker.first().unwrap().payment_hash,
        maker_init_response.payment_hash
    );
    let swaps_taker = list_swaps(taker_addr).await;
    assert_eq!(swaps_taker.taker.len(), 1);
    assert_eq!(
        swaps_taker.taker.first().unwrap().payment_hash,
        maker_init_response.payment_hash
    );

    println!("\nexecute swap");
    maker_execute(
        maker_addr,
        maker_init_response.swapstring,
        maker_init_response.payment_secret,
        node3_pubkey.clone(),
    )
    .await;

    wait_for_swap_status(
        taker_addr,
        &maker_init_response.payment_hash,
        SwapStatus::Pending,
    )
    .await;

    wait_for_ln_balance(maker_addr, &asset_id_1, 490).await;
    wait_for_ln_balance(maker_addr, &asset_id_2, 20).await;
    wait_for_ln_balance(taker_addr, &asset_id_1, 10).await;
    wait_for_ln_balance(taker_addr, &asset_id_2, 480).await;

    // Swap must actually complete on both sides. Before the fix, corrupted
    // `swap_payment: true` synthesis on the second materialization could leave
    // the middle hop with inconsistent RGB accounting; the swap still tended to
    // complete, but it left the door open for downstream failures on retries
    // that reuse the payment_hash. Guard the success condition here explicitly.
    let swaps_maker = list_swaps(maker_addr).await;
    assert_eq!(
        swaps_maker.maker.first().unwrap().status,
        SwapStatus::Succeeded
    );
    let swaps_taker = list_swaps(taker_addr).await;
    assert_eq!(
        swaps_taker.taker.first().unwrap().status,
        SwapStatus::Succeeded
    );

    println!("\nverify off-chain RGB balances on all three nodes");
    let balance_1_1 = asset_balance(node1_addr, &asset_id_1).await;
    let balance_2_1 = asset_balance(node2_addr, &asset_id_1).await;
    let balance_3_1 = asset_balance(node3_addr, &asset_id_1).await;
    let balance_1_2 = asset_balance(node1_addr, &asset_id_2).await;
    let balance_2_2 = asset_balance(node2_addr, &asset_id_2).await;
    let balance_3_2 = asset_balance(node3_addr, &asset_id_2).await;
    assert_eq!(balance_1_1.offchain_outbound, 490);
    assert_eq!(balance_1_1.offchain_inbound, 10);
    assert_eq!(balance_2_1.offchain_outbound, 300);
    assert_eq!(balance_2_1.offchain_inbound, 500);
    assert_eq!(balance_3_1.offchain_outbound, 10);
    assert_eq!(balance_3_1.offchain_inbound, 290);
    assert_eq!(balance_1_2.offchain_outbound, 20);
    assert_eq!(balance_1_2.offchain_inbound, 280);
    assert_eq!(balance_2_2.offchain_outbound, 300);
    assert_eq!(balance_2_2.offchain_inbound, 500);
    assert_eq!(balance_3_2.offchain_outbound, 480);
    assert_eq!(balance_3_2.offchain_inbound, 20);

    println!("\nverify per-channel sat deltas match expected swap fees");
    let channels_1_after = list_channels(node1_addr).await;
    let channels_2_after = list_channels(node2_addr).await;
    let channels_3_after = list_channels(node3_addr).await;
    let chan_1_12 = channels_1_after
        .iter()
        .find(|c| c.channel_id == channel_12.channel_id)
        .unwrap();
    let chan_1_21 = channels_1_after
        .iter()
        .find(|c| c.channel_id == channel_21.channel_id)
        .unwrap();
    let chan_2_12 = channels_2_after
        .iter()
        .find(|c| c.channel_id == channel_12.channel_id)
        .unwrap();
    let chan_2_23 = channels_2_after
        .iter()
        .find(|c| c.channel_id == channel_23.channel_id)
        .unwrap();
    let chan_2_32 = channels_2_after
        .iter()
        .find(|c| c.channel_id == channel_32.channel_id)
        .unwrap();
    let chan_2_21 = channels_2_after
        .iter()
        .find(|c| c.channel_id == channel_21.channel_id)
        .unwrap();
    let chan_3_23 = channels_3_after
        .iter()
        .find(|c| c.channel_id == channel_23.channel_id)
        .unwrap();
    let chan_3_32 = channels_3_after
        .iter()
        .find(|c| c.channel_id == channel_32.channel_id)
        .unwrap();
    let htlc_min_sat = HTLC_MIN_MSAT / 1000;
    let fees = 1;
    assert!(chan_1_12.local_balance_sat < chan_1_12_before.local_balance_sat - htlc_min_sat);
    assert!(
        chan_1_12.local_balance_sat
            >= chan_1_12_before.local_balance_sat - htlc_min_sat - (fees * 3)
    );
    assert_eq!(
        chan_2_12.local_balance_sat,
        chan_2_12_before.local_balance_sat + htlc_min_sat + (fees * 2)
    );
    assert_eq!(
        chan_2_23.local_balance_sat,
        chan_2_23_before.local_balance_sat - htlc_min_sat - fees
    );
    assert_eq!(
        chan_3_23.local_balance_sat,
        chan_3_23_before.local_balance_sat + htlc_min_sat + fees
    );
    assert_eq!(
        chan_3_32.local_balance_sat,
        chan_3_32_before.local_balance_sat - htlc_min_sat - fees
    );
    assert_eq!(
        chan_2_32.local_balance_sat,
        chan_2_32_before.local_balance_sat + htlc_min_sat + fees
    );
    assert_eq!(
        chan_2_21.local_balance_sat,
        chan_2_21_before.local_balance_sat - htlc_min_sat
    );
    assert_eq!(
        chan_1_21.local_balance_sat,
        chan_1_21_before.local_balance_sat + htlc_min_sat
    );

    // Per-channel RGB amounts on node 2 should line up for both contract IDs.
    // Even with the pre-fix synthesis bug these happened to be correct because
    // amounts are copied from the per-HTLC `htlc_amount_rgb` field, but we
    // check them to catch any downstream drift if the fallback ever regresses.
    assert_eq!(chan_2_12.asset_local_amount, Some(10));
    assert_eq!(chan_2_12.asset_remote_amount, Some(490));
    assert_eq!(chan_2_23.asset_local_amount, Some(290));
    assert_eq!(chan_2_23.asset_remote_amount, Some(10));
    assert_eq!(chan_2_32.asset_local_amount, Some(20));
    assert_eq!(chan_2_32.asset_remote_amount, Some(480));
    assert_eq!(chan_2_21.asset_local_amount, Some(280));
    assert_eq!(chan_2_21.asset_remote_amount, Some(20));

    // Direct KVStore assertion that distinguishes pre-fix from post-fix.
    //
    // Node 2 is a pure forwarder: nothing on node 2 ever calls
    // `write_rgb_payment_info_file`, so the writer-owned per-payment-hash
    // record at the bare key `{payment_hash_hex}` in `payment_info_inbound` /
    // `payment_info_outbound` must not exist.
    //
    // Pre-fix, `color_commitment` synthesized an `RgbPaymentInfo` with
    // `swap_payment: true` whenever the channel-scoped record was absent AND
    // the global `{payment_hash}_pending` seed had already been consumed; it
    // then wrote that synthesized record back to `{payment_hash_hex}`,
    // clobbering the permanent key. The atomic asset-for-asset swap forces
    // exactly this on node 2 because two channels in each direction share the
    // payment_hash.
    //
    // Post-fix, the synthesis branch no longer writes to `{payment_hash_hex}`,
    // and the new fallback prefers the writer-owned record (which doesn't
    // exist on a forwarder anyway, so this branch never fires on node 2). The
    // net effect: node 2's KVStore must contain zero bare-payment-hash keys in
    // either payment_info namespace. Channel-scoped records (keyed by
    // `chan_id || payment_hash`, 128 hex chars) are expected and allowed.
    let payment_hash_hex = &maker_init_response.payment_hash;
    assert_eq!(payment_hash_hex.len(), 64);
    let node2_db = Path::new(&test_dir_node2).join("rln_db");
    for namespace in &["payment_info_inbound", "payment_info_outbound"] {
        let sql = format!(
            "SELECT count(*) FROM kv_store \
             WHERE primary_namespace = 'rgb' \
             AND secondary_namespace = '{namespace}' \
             AND key = '{payment_hash_hex}';"
        );
        let out = Command::new("sqlite3")
            .arg(node2_db.as_os_str())
            .arg(&sql)
            .output()
            .expect("sqlite3 must be available on the test host");
        assert!(
            out.status.success(),
            "sqlite3 failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let count = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(
            count, "0",
            "node 2 leaked a synthesized `{namespace}` record keyed by bare \
             payment_hash {payment_hash_hex} — the pending-seed regression is \
             still present: forwarder `color_commitment` is overwriting the \
             writer-owned permanent record"
        );
    }
}
