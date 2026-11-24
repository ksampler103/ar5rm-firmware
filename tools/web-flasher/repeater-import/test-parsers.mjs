#!/usr/bin/env node
/* Unit tests for the web flasher's repeater file-import parsers.
 *
 * The parsers live in the flasher page between the RPTIMPORT-BEGIN /
 * RPTIMPORT-END markers (pure functions, no DOM). This script extracts
 * them from the page source, runs them against the fixtures in
 * ./fixtures, and verifies that parsed records feed the page's existing
 * buildRpbkBlock() to a byte-valid RPBK block — cross-checked against
 * tools/simulator/tools/make_rpbk.py (masking header byte 5: the page
 * writes source=1 for ZIP/file blocks, the Python builder writes 0).
 *
 * Page source resolution: $FLASHER_HTML, else the artifact source at
 * ~/workspace/ts-spaces/ar-5rm-web-flasher/index.html, else the repo's
 * docs/index.html (which lags the artifact until the coordinator syncs).
 *
 * Run: node tools/web-flasher/repeater-import/test-parsers.mjs
 */
import { readFileSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, '..', '..', '..');
const candidates = [
  process.env.FLASHER_HTML,
  join(repo, '..', 'ts-spaces', 'ar-5rm-web-flasher', 'index.html'),
  join(repo, 'docs', 'index.html'),
].filter(Boolean);
const pagePath = candidates.find((p) => existsSync(p));
if (!pagePath) { console.error('FAIL: no flasher page source found'); process.exit(1); }
const page = readFileSync(pagePath, 'utf8');
console.log('Page source:', pagePath);

function extractMarked(src, beginTag, endTag) {
  const b = src.indexOf(beginTag);
  const e = src.indexOf(endTag);
  if (b < 0 || e < 0 || e < b) throw new Error('markers not found: ' + beginTag);
  const start = src.lastIndexOf('/*', b);
  const endClose = src.indexOf('*/', e);
  return src.slice(start, endClose + 2);
}
function extractFunc(src, name) {
  const i = src.indexOf('function ' + name + '(');
  if (i < 0) throw new Error('function not found: ' + name);
  const j = src.indexOf('{', i);
  let depth = 0;
  for (let k = j; k < src.length; k++) {
    if (src[k] === '{') depth++;
    else if (src[k] === '}') { depth--; if (depth === 0) return src.slice(i, k + 1); }
  }
  throw new Error('unterminated function: ' + name);
}

const code = [
  extractMarked(page, 'RPTIMPORT-BEGIN', 'RPTIMPORT-END'),
  extractFunc(page, 'crc16Xmodem'),
  extractFunc(page, 'buildRpbkBlock'),
  'return { parseRepeaterFileText, buildRpbkBlock, crc16Xmodem };',
].join('\n');
const { parseRepeaterFileText, buildRpbkBlock, crc16Xmodem } = new Function(code)();

let pass = 0, fail = 0;
function check(label, cond, extra) {
  if (cond) { pass++; console.log('  ok  ' + label); }
  else { fail++; console.log('  FAIL ' + label + (extra !== undefined ? ' — got ' + JSON.stringify(extra) : '')); }
}
function eq(a, b) { return a === b; }
function fixture(name) { return readFileSync(join(here, 'fixtures', name), 'utf8'); }

/* Independent CRC-16/XMODEM (poly 0x1021, init 0) for block verification. */
function crc16(bytes) {
  let crc = 0;
  for (const b of bytes) {
    crc ^= b << 8;
    for (let i = 0; i < 8; i++) crc = (crc & 0x8000) ? ((crc << 1) ^ 0x1021) & 0xFFFF : (crc << 1) & 0xFFFF;
  }
  return crc;
}

console.log('\nRepeaterBook web-export CSV (documented "Web Export" columns, no coordinates):');
{
  const r = parseRepeaterFileText(fixture('fixture-repeaterbook-webexport.csv'));
  check('format is CSV', eq(r.format, 'CSV'), r.format);
  check('4 records, 1 skipped', r.records.length === 4 && r.skipped === 1, [r.records.length, r.skipped]);
  const [a, b, c, d] = r.records;
  check('W4WBT rx/tx from explicit columns', a.rx_hz === 146940000 && a.tx_hz === 146340000, [a.rx_hz, a.tx_hz]);
  check('W4WBT Tone mode -> TX CTCSS 141.3 only', a.tx_tone === 1413 && a.rx_tone === 0, [a.tx_tone, a.rx_tone]);
  check('W4WBT locality from Comment', a.locality === 'Gastonia', a.locality);
  check('K4GFD T SQL -> TX+RX CTCSS 100.0', b.tx_tone === 1000 && b.rx_tone === 1000, [b.tx_tone, b.rx_tone]);
  check('GMRS 15 no tone', c.tx_tone === 0 && c.rx_hz === 462562500 && c.tx_hz === 467562500, [c.tx_tone, c.rx_hz, c.tx_hz]);
  check('W4BFB DCS 023 -> code 1 both sides', d.tx_tone === 1 && d.rx_tone === 1, [d.tx_tone, d.rx_tone]);
  check('no coordinates anywhere (CSV export carries none)', r.records.every((e) => e.lat === undefined), r.records.map((e) => e.lat));
}

console.log('\nCHIRP CSV:');
{
  const r = parseRepeaterFileText(fixture('fixture-chirp.csv'));
  check('5 records, 1 skipped', r.records.length === 5 && r.skipped === 1, [r.records.length, r.skipped]);
  const [a, b, c, d, e] = r.records;
  check('duplex -0.6 -> tx 146.34M', a.tx_hz === 146340000, a.tx_hz);
  check('Tone mode -> TX tone only (1413/0)', a.tx_tone === 1413 && a.rx_tone === 0, [a.tx_tone, a.rx_tone]);
  check('quoted comma field parsed as one locality', a.locality === 'Gastonia, NC', a.locality);
  check('TSQL -> 1000/1000', b.tx_tone === 1000 && b.rx_tone === 1000, [b.tx_tone, b.rx_tone]);
  check('DTCS 205 -> code 34 both sides (0o205 is table index 33)', c.tx_tone === 34 && c.rx_tone === 34, [c.tx_tone, c.rx_tone]);
  check('empty Tone mode -> no tone despite default rToneFreq', d.tx_tone === 0 && d.rx_tone === 0 && d.tx_hz === d.rx_hz, [d.tx_tone, d.rx_tone]);
  check('1.25m -1.6 -> tx 223.06M, tone 1000', e.tx_hz === 223060000 && e.tx_tone === 1000, [e.tx_hz, e.tx_tone]);
}

console.log('\nMinimal CSV (name,freq_mhz,tone,lat,lon):');
{
  const r = parseRepeaterFileText(fixture('fixture-minimal.csv'));
  check('2 records, 1 skipped', r.records.length === 2 && r.skipped === 1, [r.records.length, r.skipped]);
  const [a, b] = r.records;
  check('numeric Tone column treated as CTCSS value', a.tx_tone === 1413, a.tx_tone);
  check('coordinates kept', a.lat === 35.2621 && a.lon === -81.1783, [a.lat, a.lon]);
  check('empty tone -> 0, simplex tx', b.tx_tone === 0 && b.tx_hz === b.rx_hz, [b.tx_tone, b.tx_hz]);
}

console.log('\nShard JSON:');
{
  const r = parseRepeaterFileText(fixture('fixture-shard.json'));
  check('format is shard JSON', eq(r.format, 'shard JSON'), r.format);
  check('3 records, 1 skipped', r.records.length === 3 && r.skipped === 1, [r.records.length, r.skipped]);
  check('fields pass through', r.records[0].rx_hz === 462600000 && r.records[0].tx_tone === 1413 && r.records[0].lat === 35.2621, r.records[0]);
  check('coordless record kept without lat/lon', r.records[2].lat === undefined && r.records[2].name === 'NO COORDS', r.records[2]);
}

console.log('\nRepeaterBook GPX (coordinates + free-text metadata):');
{
  const r = parseRepeaterFileText(fixture('fixture-repeaterbook.gpx'));
  check('format is GPX', eq(r.format, 'GPX'), r.format);
  check('3 records, 0 skipped', r.records.length === 3 && r.skipped === 0, [r.records.length, r.skipped]);
  const [a, b, c] = r.records;
  check('W4WBT: -0.6 offset, PL 141.3, coords', a.name === 'W4WBT' && a.tx_hz === 146340000 && a.tx_tone === 1413 && a.lat === 35.2621 && a.lon === -81.1783, a);
  check('K4KMT: +5 offset, PL 100.0', b.tx_hz === 448862500 && b.tx_tone === 1000, [b.tx_hz, b.tx_tone]);
  check('W4DCS: DCS 023 -> code 1 (callsign ...DCS does not false-match)', c.tx_tone === 1 && c.name === 'W4DCS', [c.tx_tone, c.name]);
}

console.log('\nBTECH CSV (RepeaterBook BTECH UV-PRO/GMRS-PRO export):');
{
  const r = parseRepeaterFileText(fixture('fixture-btech.csv'));
  check('3 records, 0 skipped', r.records.length === 3 && r.skipped === 0, [r.records.length, r.skipped]);
  const [a, b, c] = r.records;
  check('Hz frequencies + 5-digit CTCSS (14130 -> 1413)', a.rx_hz === 146940000 && a.tx_hz === 146340000 && a.tx_tone === 1413 && a.rx_tone === 0, a);
  check('DCS 023N -> code 1 both sides', b.tx_tone === 1 && b.rx_tone === 1, [b.tx_tone, b.rx_tone]);
  check('simplex, no tone', c.tx_hz === c.rx_hz && c.tx_tone === 0, [c.tx_hz, c.tx_tone]);
}

console.log('\nMalformed input handling:');
{
  let threw = null;
  try { parseRepeaterFileText('   \n'); } catch (e) { threw = e; }
  check('empty file -> clean error, no crash', !!threw && /empty/i.test(threw.message), threw && threw.message);
  threw = null;
  try { parseRepeaterFileText('a,b,c\n1,2,3\n'); } catch (e) { threw = e; }
  check('unknown CSV headers -> clean error', !!threw && /header/i.test(threw.message), threw && threw.message);
  threw = null;
  try { parseRepeaterFileText('{"nope":true}'); } catch (e) { threw = e; }
  check('JSON without repeater array -> clean error', !!threw && /repeater array/i.test(threw.message), threw && threw.message);
}

console.log('\nRPBK block from parsed records (page builder vs make_rpbk.py):');
{
  const recs = parseRepeaterFileText(fixture('fixture-shard.json')).records;
  const block = buildRpbkBlock(recs, 1);
  check('magic RPBK', String.fromCharCode(block[0], block[1], block[2], block[3]) === 'RPBK');
  check('version 1, source 1', block[4] === 1 && block[5] === 1, [block[4], block[5]]);
  check('count LE = 3', (block[6] | (block[7] << 8)) === 3, block[6]);
  const recBytes = block.slice(16);
  check('header CRC == independent CRC-16/XMODEM of records', (block[8] | (block[9] << 8)) === crc16(recBytes), block[8] | (block[9] << 8));
  check('header CRC == page crc16Xmodem of records', (block[8] | (block[9] << 8)) === crc16Xmodem(recBytes));
  check('record 0 name NUL-padded', String.fromCharCode(...block.slice(16, 28)) === 'GASTONIA 600' && block[28] === 0);
  const dv = new DataView(block.buffer, block.byteOffset);
  check('record 0 rx/tx LE', dv.getUint32(16 + 16, true) === 462600000 && dv.getUint32(16 + 20, true) === 467600000);
  check('record 0 tones LE + flags (duplex|tx tone = 3)', dv.getUint16(16 + 24, true) === 1413 && block[16 + 28] === 3, block[16 + 28]);

  const py = join(repo, 'tools', 'simulator', 'tools', 'make_rpbk.py');
  const out = join(here, '.tmp-rpbk.bin');
  execFileSync('python3', [py, out,
    'GASTONIA 600', '462600000', '467600000', '1413', '0',
    'KINGS MTN 440', '443862500', '448862500', '1000', '1000',
    'NO COORDS', '146520000', '146520000', '0', '0',
  ], { stdio: 'pipe' });
  const pyBlock = readFileSync(out);
  execFileSync('rm', ['-f', out]);
  let same = pyBlock.length === block.length;
  if (same) for (let i = 0; i < block.length; i++) { if (i === 5) continue; if (pyBlock[i] !== block[i]) { same = false; break; } }
  check('byte-identical to make_rpbk.py (except source byte 5)', same, [pyBlock.length, block.length]);
}

console.log('\n' + pass + ' passed, ' + fail + ' failed');
process.exit(fail ? 1 : 0);
