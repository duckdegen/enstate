import { randomBytes } from 'node:crypto';
import qs from 'qs';

import { Dataset } from '.';

const MAX_BULK = 10;

export const dataset_name_bulk: Dataset<
    | {
          response: ({ type: 'success' | 'error' } & ({ address: string } | { status: number }))[];
          response_length: number;
      }
    | { status: number }
> = [
    {
        label: 'Too many inputs',
        arg: qs.stringify(
            {
                names: Array.from({ length: MAX_BULK + 1 }).map((_, index) => `${index}.eth`),
            },
            { encode: false }
        ),
        expected: {
            status: 400,
        },
    },
    {
        label: 'ETHRegistry',
        arg: qs.stringify({ names: ['luc.eth', 'nick.eth'] }, { encode: false }),
        expected: {
            response: [
                { type: 'success', address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
                { type: 'success', address: '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'ETHRegistry (extra)',
        arg: qs.stringify({ names: ['luc.eth', 'nick.eth', 'nick.eth'] }, { encode: false }),
        expected: {
            response: [
                { type: 'success', address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
                { type: 'success', address: '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5' },
            ],
            response_length: 2,
        },
    },
    // {
    //     label: 'DNSRegistry',
    //     arg: qs.stringify({ names: ['luc.computer', 'luc.cash'] }, { encode: false }),
    //     expected: {
    //         response: [
    //             { type: 'success', address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    //             { type: 'success', address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    //         ],
    //         response_length: 2,
    //     },
    // },
    // {
    //     label: 'DNSRegistry (offchain DNSSEC)',
    //     arg: qs.stringify({ names: ['antony.cash', 'cold.antony.cash'] }, { encode: false }),
    //     expected: {
    //         response: [
    //             { type: 'success', address: '0x2B5c7025998f88550Ef2fEce8bf87935f542C190' },
    //             { type: 'success', address: '0x797664168c3DEffdF3Eb3Ae18b29a2c7A7156feB' },
    //         ],
    //         response_length: 2,
    //     },
    // },
    // TODO: the offchain gateway of luc.willbreak.eth (rs.myeth.id) no longer returns its records
    // {
    //     label: 'CCIP',
    //     arg: qs.stringify({ names: ['luc.willbreak.eth', 'lucemans.cb.id'] }, { encode: false }),
    //     expected: {
    //         response: [
    //             { type: 'success', address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    //             { type: 'success', address: '0x4e7abb71BEe38011c54c30D0130c0c71Da09222b' },
    //         ],
    //         response_length: 2,
    //     },
    // },
    {
        label: 'Errors (not found)',
        arg: qs.stringify(
            {
                names: [
                    randomBytes(8).toString('hex') + '.eth',
                    randomBytes(8).toString('hex') + '.com',
                ],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'error', status: 404 },
                { type: 'error', status: 404 },
            ],
            response_length: 2,
        },
    },
    {
        label: 'GNS',
        arg: qs.stringify({ names: ['gns.gwei', 'donnoh.gwei'] }, { encode: false }),
        expected: {
            response: [
                { type: 'success', address: '0xbBBCe157ecBB945b1F92Af88413b911910C727c8' },
                { type: 'success', address: '0xC04689227Fa24785609B1174698DBe481437f1A3' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'GNS and ETHRegistry',
        arg: qs.stringify({ names: ['safe.safeq.gwei', 'nick.eth'] }, { encode: false }),
        expected: {
            response: [
                { type: 'success', address: '0x16A20A40485d9dfC0789DE97E0bF9b17BA8edD23' },
                { type: 'success', address: '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'GNS errors (not found, empty label)',
        arg: qs.stringify(
            {
                names: [randomBytes(8).toString('hex') + '.gwei', 'a..gwei'],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'error', status: 404 },
                { type: 'error', status: 400 },
            ],
            response_length: 2,
        },
    },
];

export const dataset_address_bulk: Dataset<
    | {
          response: ({ type: 'success' | 'error' } & ({ name: string } | { status: number }))[];
          response_length: number;
      }
    | { status: number }
> = [
    {
        label: 'Too many inputs',
        arg: qs.stringify(
            {
                addresses: Array.from({ length: MAX_BULK + 1 }).map(
                    (_, index) => `0x${'0'.repeat(39)}${index}`
                ),
            },
            { encode: false }
        ),
        expected: {
            status: 400,
        },
    },
    {
        label: 'ETHRegistry',
        arg: qs.stringify(
            {
                addresses: [
                    '0x225f137127d9067788314bc7fcc1f36746a3c3B5',
                    '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5',
                ],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', name: 'luc.eth' },
                { type: 'success', name: 'nick.eth' },
            ],
            response_length: 2,
        },
    },
    // {
    //     label: 'ETHRegistry (extra)',
    //     arg: qs.stringify(
    //         {
    //             addresses: [
    //                 '0x2B5c7025998f88550Ef2fEce8bf87935f542C190',
    //                 '0x2B5c7025998f88550Ef2fEce8bf87935F542c190',
    //             ],
    //         },
    //         { encode: false }
    //     ),
    //     expected: { response: [{ type: 'success', name: 'luc.cash' }], response_length: 1 },
    // },
    {
        label: 'DNSRegistry',
        arg: qs.stringify(
            { addresses: ['0x225f137127d9067788314bc7fcc1f36746a3c3B5'] },
            { encode: false }
        ),
        expected: { response: [{ type: 'success', name: 'luc.eth' }], response_length: 1 },
    },
    {
        label: 'Errors (invalid address)',
        arg: qs.stringify(
            {
                addresses: ['hi'],
            },
            { encode: false }
        ),
        expected: {
            status: 400,
        },
    },
    {
        label: 'Errors (not found)',
        arg: qs.stringify(
            {
                addresses: ['0x000000000000000000000000000000000000ffff'],
            },
            { encode: false }
        ),
        expected: {
            response: [{ type: 'error', status: 404 }],
            response_length: 1,
        },
    },
    // {
    //     label: 'CCIP',
    //     arg: qs.stringify(
    //         { names: ['luc.willbreak.eth', 'lucemans.cb.id'] },
    //         { encode: false }
    //     ),
    //     expected: {
    //         response: [
    //             { name: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    //             { name: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
    //         ],
    //         response_length: 2,
    //     },
    // },
    {
        label: 'GNS',
        arg: qs.stringify(
            {
                addresses: [
                    '0xC04689227Fa24785609B1174698DBe481437f1A3',
                    '0x881F5d0a21E8257C5a6a9137cDb09391df153889',
                ],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', name: 'donnoh.gwei' },
                { type: 'success', name: 'zodomo.gwei' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'GNS and ETHRegistry',
        arg: qs.stringify(
            {
                addresses: [
                    '0xDDCd43a2906b210Dd21b9c32260Dc655EB3164Ff',
                    '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5',
                ],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', name: 'tatsumaki.gwei' },
                { type: 'success', name: 'nick.eth' },
            ],
            response_length: 2,
        },
    },
];

export const dataset_universal_bulk: Dataset<
    | {
          response: ({ type: 'success' | 'error' } & (
              | { address: string }
              | { name: string }
              | {
                    status: number;
                }
          ))[];
          response_length: number;
      }
    | { status: number }
> = [
    {
        label: 'Too many inputs',
        arg: qs.stringify(
            { queries: Array.from({ length: MAX_BULK + 1 }).map((_, index) => `${index}.eth`) },
            { encode: false }
        ),
        expected: {
            status: 400,
        },
    },
    {
        label: 'ETHRegistry',
        arg: qs.stringify(
            {
                queries: ['luc.eth', '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5'],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
                { type: 'success', name: 'nick.eth' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'DNSRegistry',
        arg: qs.stringify(
            {
                queries: ['0x225f137127d9067788314bc7fcc1f36746a3c3B5', 'luc.eth'],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', name: 'luc.eth' },
                { type: 'success', address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'Mixed',
        arg: qs.stringify(
            {
                // TODO: luc.willbreak.eth was the third query, but its offchain
                // gateway (rs.myeth.id) no longer returns its records
                queries: ['0x225f137127d9067788314bc7fcc1f36746a3c3B5', 'luc.eth'],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', name: 'luc.eth' },
                { type: 'success', address: '0x225f137127d9067788314bc7fcc1f36746a3c3B5' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'Errors',
        arg: qs.stringify(
            {
                queries: [
                    '0x000000000000000000000000000000000000ffff',
                    randomBytes(8).toString('hex') + '.eth',
                    randomBytes(8).toString('hex') + '.com',
                ],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'error', status: 404 },
                { type: 'error', status: 404 },
                { type: 'error', status: 404 },
            ],
            response_length: 3,
        },
    },
    {
        label: 'GNS and ETHRegistry',
        arg: qs.stringify(
            {
                queries: [
                    '0xC04689227Fa24785609B1174698DBe481437f1A3',
                    'donnoh.gwei',
                    'nick.eth',
                    '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5',
                ],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', name: 'donnoh.gwei' },
                { type: 'success', address: '0xC04689227Fa24785609B1174698DBe481437f1A3' },
                { type: 'success', address: '0xb8c2C29ee19D8307cb7255e1Cd9CbDE883A267d5' },
                { type: 'success', name: 'nick.eth' },
            ],
            response_length: 4,
        },
    },
];
