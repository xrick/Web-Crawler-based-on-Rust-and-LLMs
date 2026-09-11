#!/usr/bin/env python3
"""Run against the local server. Saves an auditable API snapshot; no cloud LLM is used."""
from collections import Counter
import argparse
import json
import os
import time
import urllib.request
from pathlib import Path

BASE = 'http://127.0.0.1:8080'

def request(path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, data=data, headers={'Content-Type': 'application/json', 'X-Crawler-Request': '1'})
    with urllib.request.urlopen(req, timeout=15) as response:
        return json.load(response)

def validate_job(job, mode):
    """Return a machine-readable quality report; never accept empty coverage."""
    errors = []
    categories = set(job['settings']['categories'])
    products = job['products']
    candidates = job['candidates']
    statuses = Counter(spec['status'] for p in products for spec in p['specs'])
    if job['status'] not in ('completed', 'completed_with_errors'):
        errors.append('Job did not finish successfully')
    expected_options = (mode == 'discovery', 1 if mode == 'sample' else 0)
    actual_options = (job['options']['discovery_only'], job['options']['products_per_category'])
    if actual_options != expected_options:
        errors.append('Requested verification mode does not match job options')
    discovered = {c['category'] for c in candidates if c.get('specs_url')}
    if categories - discovered:
        errors.append('Missing specification links for: ' + ', '.join(sorted(categories - discovered)))
    if mode != 'discovery':
        covered = {p['category'] for p in products}
        if not products or categories - covered:
            errors.append('Missing extracted products for: ' + ', '.join(sorted(categories - covered)))
        urls = [p['product_url'] for p in products]
        if len(urls) != len(set(urls)):
            errors.append('Duplicate extracted products')
        if mode == 'full':
            missing = {c['url'] for c in candidates} - set(urls)
            if missing:
                errors.append(f'{len(missing)} discovered products were not extracted')
        for product in products:
            blocks = {b['id']: b for b in product['blocks']}
            ids = [s['block_id'] for s in product['specs']]
            if not blocks or len(blocks) != len(product['blocks']) or len(ids) != len(set(ids)) or set(ids) != set(blocks):
                errors.append(product['product_url'] + ': source block coverage is incomplete or duplicated')
            for spec in product['specs']:
                block = blocks.get(spec['block_id'])
                if block is None or spec['value'] != block['text'] or spec['evidence'] != block['text']:
                    errors.append(product['product_url'] + ': source text mismatch')
            if not any(s['status'] in ('source_matched', 'needs_review') for s in product['specs']):
                errors.append(product['product_url'] + ': no model-classified specifications')
        if statuses['llm_failed']:
            errors.append(f"{statuses['llm_failed']} specification blocks failed classification")
    return {'passed': not errors, 'errors': errors, 'spec_statuses': dict(statuses),
            'status': job['status'], 'products': len(products), 'pages': len(job['pages']),
            'issues': len(job['issues'])}


def main():
    global BASE
    parser = argparse.ArgumentParser()
    parser.add_argument('--mode', choices=['discovery', 'sample', 'full'], default='sample')
    parser.add_argument('--output-dir', type=Path, default=Path(os.environ.get('CRAWLER_DATA_DIR', 'crawler_data/apple')) / 'verification', help='Directory for job snapshots and quality reports')
    parser.add_argument('--base-url', default=BASE, help='Local crawler server URL')
    parser.add_argument('--job', help='Observe an existing job instead of starting one')
    args = parser.parse_args()
    BASE = args.base_url.rstrip('/')
    job = request('/api/jobs/' + args.job) if args.job else request('/api/jobs', {
        'discovery_only': args.mode == 'discovery',
        'products_per_category': 1 if args.mode == 'sample' else 0,
    })
    print('Job:', job['id'], flush=True)
    last = None
    while job['status'] in ('running', 'cancelling'):
        signature = (job['phase'], len(job['pages']), job['succeeded'])
        if signature != last:
            print(signature, flush=True)
            last = signature
        time.sleep(2)
        job = request('/api/jobs/' + job['id'])
    root = args.output_dir
    root.mkdir(parents=True, exist_ok=True)
    (root / f"{job['id']}.json").write_text(json.dumps(job, ensure_ascii=False, indent=2))
    report = validate_job(job, args.mode)
    (root / f"{job['id']}-quality.json").write_text(json.dumps(report, ensure_ascii=False, indent=2))
    print(json.dumps(report, ensure_ascii=False), flush=True)
    if not report['passed']:
        raise SystemExit(1)

if __name__ == '__main__':
    main()
