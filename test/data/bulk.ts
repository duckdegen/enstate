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
                names: Array.from({ length: MAX_BULK + 1 }).map((_, index) => `${index}.gwei`),
            },
            { encode: false }
        ),
        expected: {
            status: 400,
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
        label: 'GNS (extra)',
        arg: qs.stringify({ names: ['gns.gwei', 'donnoh.gwei', 'donnoh.gwei'] }, { encode: false }),
        expected: {
            response: [
                { type: 'success', address: '0xbBBCe157ecBB945b1F92Af88413b911910C727c8' },
                { type: 'success', address: '0xC04689227Fa24785609B1174698DBe481437f1A3' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'GNS (subdomain)',
        arg: qs.stringify({ names: ['safe.safeq.gwei', 'tatsumaki.gwei'] }, { encode: false }),
        expected: {
            response: [
                { type: 'success', address: '0x16A20A40485d9dfC0789DE97E0bF9b17BA8edD23' },
                { type: 'success', address: '0xDDCd43a2906b210Dd21b9c32260Dc655EB3164Ff' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'Errors (not found, not a .gwei name)',
        arg: qs.stringify(
            {
                names: [randomBytes(8).toString('hex') + '.gwei', 'luc.eth'],
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
        label: 'GNS',
        arg: qs.stringify(
            {
                addresses: [
                    '0xC04689227Fa24785609B1174698DBe481437f1A3',
                    '0xDDCd43a2906b210Dd21b9c32260Dc655EB3164Ff',
                ],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', name: 'donnoh.gwei' },
                { type: 'success', name: 'tatsumaki.gwei' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'GNS (extra)',
        arg: qs.stringify(
            {
                addresses: [
                    '0xC04689227Fa24785609B1174698DBe481437f1A3',
                    '0xc04689227fa24785609b1174698dbe481437f1a3',
                ],
            },
            { encode: false }
        ),
        expected: { response: [{ type: 'success', name: 'donnoh.gwei' }], response_length: 1 },
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
            { queries: Array.from({ length: MAX_BULK + 1 }).map((_, index) => `${index}.gwei`) },
            { encode: false }
        ),
        expected: {
            status: 400,
        },
    },
    {
        label: 'GNS',
        arg: qs.stringify(
            {
                queries: ['gns.gwei', '0xDDCd43a2906b210Dd21b9c32260Dc655EB3164Ff'],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', address: '0xbBBCe157ecBB945b1F92Af88413b911910C727c8' },
                { type: 'success', name: 'tatsumaki.gwei' },
            ],
            response_length: 2,
        },
    },
    {
        label: 'Mixed',
        arg: qs.stringify(
            {
                queries: [
                    '0xC04689227Fa24785609B1174698DBe481437f1A3',
                    'donnoh.gwei',
                    'safe.safeq.gwei',
                ],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'success', name: 'donnoh.gwei' },
                { type: 'success', address: '0xC04689227Fa24785609B1174698DBe481437f1A3' },
                { type: 'success', address: '0x16A20A40485d9dfC0789DE97E0bF9b17BA8edD23' },
            ],
            response_length: 3,
        },
    },
    {
        label: 'Errors',
        arg: qs.stringify(
            {
                queries: [
                    '0x000000000000000000000000000000000000ffff',
                    randomBytes(8).toString('hex') + '.gwei',
                    'luc.eth',
                ],
            },
            { encode: false }
        ),
        expected: {
            response: [
                { type: 'error', status: 404 },
                { type: 'error', status: 404 },
                { type: 'error', status: 400 },
            ],
            response_length: 3,
        },
    },
];
