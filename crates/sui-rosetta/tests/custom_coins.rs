// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(dead_code)]
mod rosetta_client;
#[path = "custom_coins/test_coin_utils.rs"]
mod test_coin_utils;

use serde_json::json;
use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockResponseOptions};
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, Currency, NetworkIdentifier,
    SuiEnv,
};
use sui_rosetta::SUI;
use test_cluster::TestClusterBuilder;
use test_coin_utils::{init_package, mint};

use crate::rosetta_client::{start_rosetta_test_server, RosettaEndpoint};

#[tokio::test]
async fn test_custom_coin_balance() {
    // mint coins to `test_culset.get_address_1()` and `test_culset.get_address_2()`
    const SUI_BALANCE: u64 = 150_000_000_000_000_000;
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(&client, keystore, sender).await.unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    let _mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    // setup AccountBalanceRequest
    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };

    let sui_currency = SUI.clone();
    let test_coin_currency = Currency {
        coin_type: coin_type.clone(),
        symbol: "TEST_COIN".to_string(),
        decimals: 6,
    };

    // Verify initial balance and stake
    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address: address1,
            sub_account: None,
        },
        block_identifier: Default::default(),
        currencies: vec![sui_currency, test_coin_currency],
    };

    println!(
        "request: {}",
        serde_json::to_string_pretty(&request).unwrap()
    );
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    println!(
        "response: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );
    assert_eq!(response.balances.len(), 2);
    assert_eq!(response.balances[0].value, SUI_BALANCE as i128);
    assert_eq!(response.balances[0].currency.coin_type, "0x2::sui::SUI");
    assert_eq!(response.balances[1].value, COIN1_BALANCE as i128);
    assert_eq!(response.balances[1].currency.coin_type, coin_type);
}

#[tokio::test]
async fn test_custom_coin_transfer() {
    const COIN1_BALANCE: u64 = 100_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(&client, keystore, sender).await.unwrap();

    let sender = test_cluster.get_address_1();
    let recipient = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, sender)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);
    let _mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": "50000000" },
            "currency": {
                "coin_type": coin_type.clone(),
                "symbol": "TEST_COIN",
                "decimals": 6,
            }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-50000000" },
            "currency": {
                "coin_type": coin_type.clone(),
                "symbol": "TEST_COIN",
                "decimals": 6,
            }
        }]
    )).unwrap();

    let response = rosetta_client.rosetta_flow(&ops, keystore).await;

    let tx = client
        .read_api()
        .get_transaction_with_options(
            response.transaction_identifier.hash,
            SuiTransactionBlockResponseOptions::new()
                .with_input()
                .with_effects()
                .with_balance_changes()
                .with_events(),
        )
        .await
        .unwrap();

    assert_eq!(
        &SuiExecutionStatus::Success,
        tx.effects.as_ref().unwrap().status()
    );
    println!("Sui TX: {tx:?}");

    let ops2 = Operations::try_from(tx).unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );
}
