use super::*;

const TEST_DIR_BASE: &str = "tmp/force_close_pending_htlc_sweep/";

async fn spendable_sats(node_address: SocketAddr) -> u64 {
    let balance = btc_balance(node_address).await;
    balance.vanilla.spendable + balance.colored.spendable
}

async fn refresh_transfers_tolerant(node_address: SocketAddr) {
    let payload = RefreshRequest {
        asset_id: None,
        filter: vec![],
        skip_sync: false,
    };
    let _ = reqwest::Client::new()
        .post(format!("http://{node_address}/refreshtransfers"))
        .json(&payload)
        .send()
        .await;
}

fn node_ldk_logs(node_test_dir: &str) -> String {
    let log_path = Path::new(node_test_dir)
        .join(LDK_DIR)
        .join(LOGS_DIR)
        .join(LDK_LOGS_FILE);
    std::fs::read_to_string(log_path).unwrap_or_default()
}

/// Returns the txid of the confirmed commitment for the given channel.
async fn confirmed_commitment_txid(node_test_dir: &str, channel_id: &str) -> String {
    let needle = format!("Channel {channel_id} closed by funding output spend in txid ");
    let t_0 = OffsetDateTime::now_utc();
    loop {
        let logs = node_ldk_logs(node_test_dir);
        if let Some(pos) = logs.find(&needle) {
            return logs[pos + needle.len()..pos + needle.len() + 64].to_string();
        }
        if (OffsetDateTime::now_utc() - t_0).as_seconds_f32() > 30.0 {
            panic!("confirmed commitment for channel {channel_id} not seen in logs");
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

fn tx_output_sats(txid: &str) -> Vec<u64> {
    let output = Command::new("docker")
        .stdin(Stdio::null())
        .arg("compose")
        .args(bitcoin_cli())
        .arg("getrawtransaction")
        .arg(txid)
        .arg("true")
        .output()
        .expect("able to call getrawtransaction");
    assert!(output.status.success());
    let tx: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid tx JSON");
    tx["vout"]
        .as_array()
        .expect("vout array")
        .iter()
        .map(|v| {
            Amount::from_btc(v["value"].as_f64().expect("output value"))
                .expect("valid amount")
                .to_sat()
        })
        .collect()
}

/// Force close with a pending RGB HTLC (held via a hodl invoice): both nodes
/// must still recover their BTC and node1 its assets, despite the confirmed
/// commitment carrying the asset HTLC.
#[serial_test::serial]
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[traced_test]
async fn force_close_pending_htlc_sweep() {
    initialize();

    let test_dir_node1 = format!("{TEST_DIR_BASE}node1");
    let test_dir_node2 = format!("{TEST_DIR_BASE}node2");
    let (node1_addr, _) = start_node(&test_dir_node1, NODE1_PEER_PORT, false).await;
    let (node2_addr, _) = start_node(&test_dir_node2, NODE2_PEER_PORT, false).await;

    fund_and_create_utxos(node1_addr, None).await;
    fund_and_create_utxos(node2_addr, None).await;

    let asset_id = issue_asset_nia(node1_addr).await.asset_id;

    let node1_pubkey = node_info(node1_addr).await.pubkey;
    let node2_pubkey = node_info(node2_addr).await.pubkey;

    let channel = open_channel(
        node1_addr,
        &node2_pubkey,
        Some(NODE2_PEER_PORT),
        Some(100000),
        Some(50000000),
        Some(600),
        Some(&asset_id),
    )
    .await;
    wait_for_usable_channels(node1_addr, 1).await;
    wait_for_usable_channels(node2_addr, 1).await;

    // Baselines before the close: sweeps can confirm while close_channel is
    // still mining the 144 maturity blocks.
    let node1_spendable_before = spendable_sats(node1_addr).await;
    let node2_spendable_before = spendable_sats(node2_addr).await;

    // Hodl invoice: node2 holds the 10-asset HTLC without claiming it.
    let (_preimage, payment_hash) = random_preimage_and_hash();
    let LNInvoiceResponse { invoice } = ln_invoice_hodl(
        node2_addr,
        Some(HTLC_MIN_MSAT),
        Some(&asset_id),
        Some(10),
        3600,
        Some(payment_hash.clone()),
    )
    .await;
    let _ = send_payment_with_status(node1_addr, invoice, HTLCStatus::Pending).await;
    wait_for_ln_payment(node2_addr, &payment_hash, HTLCStatus::Claimable).await;

    // Force close from node2 with the HTLC held: its commitment carries it.
    close_channel(node2_addr, &channel.channel_id, &node1_pubkey, true).await;
    let commitment_txid = confirmed_commitment_txid(&test_dir_node1, &channel.channel_id).await;
    assert!(
        tx_output_sats(&commitment_txid).contains(&(HTLC_MIN_MSAT / 1000)),
        "confirmed commitment must carry the pending HTLC output"
    );

    let mut node1_btc_ok = false;
    let mut node2_btc_ok = false;
    let mut node1_assets_ok = false;
    for _ in 0..60 {
        mine_n_blocks(false, 10);
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        refresh_transfers_tolerant(node1_addr).await;
        refresh_transfers_tolerant(node2_addr).await;
        if !node1_btc_ok && spendable_sats(node1_addr).await > node1_spendable_before + 30_000 {
            node1_btc_ok = true;
        }
        if !node2_btc_ok && spendable_sats(node2_addr).await > node2_spendable_before + 30_000 {
            node2_btc_ok = true;
        }
        let node1_assets = asset_balance_spendable(node1_addr, &asset_id).await;
        if node1_assets >= 990 {
            node1_assets_ok = true;
        }
        println!(
            "recovery: node1_btc_ok={node1_btc_ok} node2_btc_ok={node2_btc_ok} node1_assets={node1_assets}"
        );
        if node1_btc_ok && node2_btc_ok && node1_assets_ok {
            // The payment was never claimed: node2 must have no assets.
            assert_eq!(asset_balance_spendable(node2_addr, &asset_id).await, 0);
            return;
        }
    }
    panic!(
        "recovery failed: node1_btc_ok={node1_btc_ok} node2_btc_ok={node2_btc_ok} node1_assets_ok={node1_assets_ok}"
    );
}
