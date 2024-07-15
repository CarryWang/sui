// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { execSync } from 'child_process';
import {
	DevInspectResults,
	SuiObjectChangePublished,
	SuiTransactionBlockResponse,
	getFullnodeUrl,
	SuiClient
} from '../../typescript/src/client/index.js';
import {
	FaucetRateLimitError,
	getFaucetHost,
	requestSuiFromFaucetV0
} from '../../typescript/src/faucet';
import { Ed25519Keypair } from '../../typescript/src/keypairs/ed25519';
import { Transaction } from '../../typescript/src/transactions';
import { retry } from 'ts-retry-promise';
import { expect } from 'vitest';
import path from "path";


const DEFAULT_FAUCET_URL = process.env.VITE_FAUCET_URL ?? getFaucetHost('localnet');
const DEFAULT_FULLNODE_URL = process.env.VITE_FULLNODE_URL ?? getFullnodeUrl('localnet');
const SUI_BIN = process.env.VITE_SUI_BIN ?? 'cargo run --bin sui';

export const DEFAULT_TICK_SIZE = 1n;
export const DEFAULT_LOT_SIZE = 1n;

export class TestToolbox {
	keypair: Ed25519Keypair;
	client: SuiClient;

	constructor(keypair: Ed25519Keypair, client: SuiClient) {
		this.keypair = keypair;
		this.client = client;
	}

	address() {
		return this.keypair.getPublicKey().toSuiAddress();
	}

	public async getActiveValidators() {
		return (await this.client.getLatestSuiSystemState()).activeValidators;
	}
}

export function getClient(): SuiClient {
	return new SuiClient({
		url: DEFAULT_FULLNODE_URL,
	});
}

// TODO: expose these testing utils from @mysten/sui
export async function setupSuiClient() {
	const keypair = Ed25519Keypair.generate();
	const address = keypair.getPublicKey().toSuiAddress();
	const client = getClient();
	await retry(() => requestSuiFromFaucetV0({ host: DEFAULT_FAUCET_URL, recipient: address }), {
		backoff: 'EXPONENTIAL',
		// overall timeout in 60 seconds
		timeout: 1000 * 60,
		// skip retry if we hit the rate-limit error
		retryIf: (error: any) => !(error instanceof FaucetRateLimitError),
		logger: (msg) => console.warn('Retrying requesting from faucet: ' + msg),
	});
	return new TestToolbox(keypair, client);
}

// TODO: expose these testing utils from @mysten/sui
export async function publishPackage(packagePath: string, toolbox?: TestToolbox) {
	// TODO: We create a unique publish address per publish, but we really could share one for all publishes.
	if (!toolbox) {
		toolbox = await setupSuiClient();
	}

	const { modules, dependencies } = JSON.parse(
		execSync(
			`${SUI_BIN} move build --dump-bytecode-as-base64 --path ${packagePath}`,
			{ encoding: 'utf-8' },
		),
	);
	const tx = new Transaction();
	const cap = tx.publish({
		modules,
		dependencies,
	});

	// Transfer the upgrade capability to the sender so they can upgrade the package later if they want.
	tx.transferObjects([cap], await toolbox.address());

	const { digest } = await toolbox.client.signAndExecuteTransaction({
		transaction: tx,
		signer: toolbox.keypair,
	});

	const publishTxn = await toolbox.client.waitForTransaction({
		digest: digest,
		options: { showObjectChanges: true, showEffects: true },
	});

	expect(publishTxn.effects?.status.status).toEqual('success');

	const packageId = ((publishTxn.objectChanges?.filter(
		(a) => a.type === 'published',
	) as SuiObjectChangePublished[]) ?? [])[0]?.packageId.replace(/^(0x)(0+)/, '0x') as string;

	expect(packageId).toBeTypeOf('string');

	console.info(`Published package ${packageId} from address ${toolbox.address()}}`);

	return { packageId, publishTxn };
}

export const main = async () => {
	const tokenSourcesPath = path.join(__dirname, "../../deepbookv3/packages/token");
	await publishPackage(tokenSourcesPath, await setupSuiClient());
}

main();