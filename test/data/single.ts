import { randomBytes } from 'node:crypto';

import { Dataset } from '.';

export const dataset_name_single: Dataset<{ address: string } | { status: number }> = [
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
        label: 'Error (not a .gwei name)',
        arg: 'luc.eth',
        expected: { status: 400 },
    },
    {
        label: 'Error (empty label)',
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
        label: 'GNS',
        arg: '0xC04689227Fa24785609B1174698DBe481437f1A3',
        expected: { name: 'donnoh.gwei' },
    },
    {
        label: 'GNS',
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
    {
        label: 'Error (not a .gwei name)',
        arg: 'luc.eth',
        expected: { status: 400 },
    },
];
