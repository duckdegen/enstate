import { randomBytes } from 'node:crypto';

import { Dataset } from '.';

export const dataset_name_single: Dataset<{ address: string } | { status: number }> = [
    {
        label: 'ETHRegistry',
        arg: 'luc.eth',
        expected: { address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    },
    {
        label: 'ETHRegistry',
        arg: 'nick.eth',
        expected: { address: '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5' },
    },
    {
        label: 'ETHRegistry (not found)',
        arg: randomBytes(8).toString('hex') + '.eth',
        expected: { status: 404 },
    },
    {
        label: 'DNSRegistry',
        arg: 'luc.computer',
        expected: { address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    },
    // {
    //     label: 'DNSRegistry',
    //     arg: 'luc.cash',
    //     expected: { address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    // },
    // {
    //     label: 'DNSRegistry (offchain DNSSEC)',
    //     arg: 'luc.cash',
    //     expected: { address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    // },
    {
        label: 'DNSRegistry (not found)',
        arg: randomBytes(8).toString('hex') + '.com',
        expected: { status: 404 },
    },
    {
        label: 'CCIP Offchain RS',
        arg: 'luc.willbreak.eth',
        expected: { address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    },
    // {
    //     label: 'CCIP Coinbase',
    //     arg: 'lucemans.cb.id',
    //     expected: { address: '0x4e7abb71BEe38011c54c30D0130c0c71Da09222b' },
    // },
    {
        label: 'GNS',
        arg: 'gns.gwei',
        expected: { address: '0xbBBCe157ecBB945b1F92Af88413b911910C727c8' },
    },
    {
        label: 'GNS',
        arg: 'donnoh.gwei',
        expected: { address: '0xC04689227Fa24785609B1174698DBe481437f1A3' },
    },
    {
        label: 'GNS (case insensitive)',
        arg: 'GNS.gwei',
        expected: { address: '0xbBBCe157ecBB945b1F92Af88413b911910C727c8' },
    },
    {
        label: 'GNS (subdomain)',
        arg: 'safe.safeq.gwei',
        expected: { address: '0x16A20A40485d9dfC0789DE97E0bF9b17BA8edD23' },
    },
    {
        label: 'GNS (not found)',
        arg: randomBytes(8).toString('hex') + '.gwei',
        expected: { status: 404 },
    },
    {
        label: 'GNS (subdomain not found)',
        arg: randomBytes(8).toString('hex') + '.gns.gwei',
        expected: { status: 404 },
    },
    {
        label: 'GNS (empty label)',
        arg: 'a..gwei',
        expected: { status: 400 },
    },
];

export const dataset_address_single: Dataset<{ name: string } | { status: number }> = [
    {
        label: 'Error (bad address)',
        arg: 'hi',
        expected: { status: 400 },
    },
    {
        label: 'Error (address not found)',
        arg: '0x000000000000000000000000000000000000ffff',
        expected: { status: 404 },
    },
    {
        label: 'ETHRegistry',
        arg: '0x225f137127d9067788314bc7fcc1f36746a3c3B5',
        expected: { name: 'luc.eth' },
    },
    {
        label: 'ETHRegistry',
        arg: '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5',
        expected: { name: 'nick.eth' },
    },
    // TODO: find another dns primary name address
    // {
    //     label: 'DNSRegistry',
    //     arg: '0x225f137127d9067788314bc7fcc1f36746a3c3B5',
    //     expected: { name: 'luc.computer' },
    // },
    // TODO: find 2 ccip primary name addresses
    // {
    //     label: 'CCIP Offchain RS',
    //     arg: '0x225f137127d9067788314bc7fcc1f36746a3c3B5',
    //     expected: { name: 'luc.willbreak.eth' },
    // },
    // {
    //     label: 'CCIP Coinbase',
    //     arg: '0x4e7abb71BEe38011c54c30D0130c0c71Da09222b',
    //     expected: { name: 'lucemans.cb.id' },
    // },
    {
        label: 'GNS',
        arg: '0xC04689227Fa24785609B1174698DBe481437f1A3',
        expected: { name: 'donnoh.gwei' },
    },
    {
        label: 'GNS',
        arg: '0x881F5d0a21E8257C5a6a9137cDb09391df153889',
        expected: { name: 'zodomo.gwei' },
    },
    {
        // also has tatsumaki.eth on ENS, the .gwei name is preferred
        label: 'GNS and ETHRegistry',
        arg: '0xDDCd43a2906b210Dd21b9c32260Dc655EB3164Ff',
        expected: { name: 'tatsumaki.gwei' },
    },
    {
        // gns.gwei resolves to this address, but the address did not pick it
        label: 'GNS (no primary name)',
        arg: '0xbBBCe157ecBB945b1F92Af88413b911910C727c8',
        expected: { status: 404 },
    },
];

export const dataset_universal_single: Dataset<
    { address: string } | { name: string } | { status: number }
> = [
    {
        label: 'Error (not found)',
        arg: '0x000000000000000000000000000000000000ffff',
        expected: { status: 404 },
    },
    {
        label: 'ETHRegistry',
        arg: 'luc.eth',
        expected: { address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    },
    {
        label: 'ETHRegistry',
        arg: '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5',
        expected: { name: 'nick.eth' },
    },
    {
        label: 'ETHRegistry (not found)',
        arg: randomBytes(8).toString('hex') + '.eth',
        expected: { status: 404 },
    },
    {
        label: 'DNSRegistry',
        arg: 'luc.computer',
        expected: { address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    },
    // {
    //     label: 'DNSRegistry (offchain DNSSEC)',
    //     arg: 'luc.cash',
    //     expected: { address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    // },
    {
        label: 'DNSRegistry (not found)',
        arg: randomBytes(8).toString('hex') + '.com',
        expected: { status: 404 },
    },
    {
        label: 'CCIP Offchain RS',
        arg: 'luc.willbreak.eth',
        expected: { address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    },
    // TODO: refer to above todo
    // {
    //     label: 'CCIP Coinbase',
    //     arg: '0x4e7abb71BEe38011c54c30D0130c0c71Da09222b',
    //     expected: { name: 'lucemans.cb.id' },
    // },
    {
        label: 'GNS',
        arg: 'gns.gwei',
        expected: { address: '0xbBBCe157ecBB945b1F92Af88413b911910C727c8' },
    },
    {
        label: 'GNS',
        arg: '0xC04689227Fa24785609B1174698DBe481437f1A3',
        expected: { name: 'donnoh.gwei' },
    },
    {
        label: 'GNS (not found)',
        arg: randomBytes(8).toString('hex') + '.gwei',
        expected: { status: 404 },
    },
];
