#!/usr/bin/env python3
"""Run against the local server. Saves an auditable API snapshot; no cloud LLM is used."""
import argparse
import json
import time
import urllib.request
from pathlib import Path

BASE = 'http://127.0.0.1:8080'

def request(path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, data=data, headers={'Content-Type': 'application/json', 'X-Crawler-Request': '1'})
    with urllib.request.urlopen(req, timeout=15) as response:
        return json.load(response)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--mode', choices=['discovery', 'sample', 'full'], default='sample')
    parser.add_argument('--job', help='Observe an existing job instead of starting one')
    args = parser.parse_args()
    job = request('/api/jobs/' + args.job) if args.job else request('/api/jobs', {
        'discovery_only': args.mode == 'discovery',
        'products_per_category': 1 if args.mode == 'sample' else 0,
    })
    print('Job:', job['id'], flush=True)
    last = None
    while job['status'] == 'running':
        signature = (job['phase'], len(job['pages']), job['succeeded'])
        if signature != last:
            print(signature, flush=True)
            last = signature
        time.sleep(2)
        job = request('/api/jobs/' + job['id'])
    root = Path('output/verification')
    root.mkdir(parents=True, exist_ok=True)
    (root / f"{job['id']}.json").write_text(json.dumps(job, ensure_ascii=False, indent=2))
    for product in job['products']:
        blocks = {b['id']: b for b in product['blocks']}
        assert len(product['specs']) == len(blocks), 'Some source blocks were lost'
        for spec in product['specs']:
            assert spec['value'] == blocks[spec['block_id']]['text'] == spec['evidence']
    print(json.dumps({'status': job['status'], 'products': len(job['products']), 'pages': len(job['pages']), 'issues': len(job['issues'])}, ensure_ascii=False), flush=True)
    if job['status'] in ('failed', 'cancelled', 'interrupted'):
        raise SystemExit(1)

if __name__ == '__main__':
    main()
