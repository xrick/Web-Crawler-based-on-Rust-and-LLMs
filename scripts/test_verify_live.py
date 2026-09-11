import copy
import unittest
from verify_live import validate_job


def sample():
    return {'status': 'completed', 'settings': {'categories': ['iphone']},
            'options': {'discovery_only': False, 'products_per_category': 1},
            'candidates': [{'category': 'iphone', 'url': 'product', 'specs_url': 'specs'}],
            'products': [{'category': 'iphone', 'product_url': 'product',
                          'blocks': [{'id': 0, 'text': '256GB'}, {'id': 1, 'text': '512GB'}],
                          'specs': [{'block_id': i, 'value': text, 'evidence': text, 'status': 'source_matched'}
                                    for i, text in enumerate(['256GB', '512GB'])]}],
            'pages': [], 'issues': []}


class VerificationTests(unittest.TestCase):
    def test_valid_sample(self):
        self.assertTrue(validate_job(sample(), 'sample')['passed'])

    def test_empty_missing_duplicate_and_failed_results(self):
        for change in ('empty', 'category', 'duplicate', 'fallback', 'text', 'blocks'):
            with self.subTest(change=change):
                j = sample()
                if change == 'empty': j['products'] = []
                if change == 'category': j['settings']['categories'].append('mac')
                if change == 'duplicate': j['products'][0]['specs'][1] = copy.deepcopy(j['products'][0]['specs'][0])
                if change == 'fallback':
                    for s in j['products'][0]['specs']: s['status'] = 'llm_failed'
                if change == 'text': j['products'][0]['specs'][0]['value'] = 'invented'
                if change == 'blocks': j['products'][0]['blocks'][1]['id'] = 0
                self.assertFalse(validate_job(j, 'sample')['passed'])

    def test_discovery_requires_links_but_not_products(self):
        j = sample()
        j['options'] = {'discovery_only': True, 'products_per_category': 0}
        j['products'] = []
        self.assertTrue(validate_job(j, 'discovery')['passed'])
        j['candidates'][0]['specs_url'] = None
        self.assertFalse(validate_job(j, 'discovery')['passed'])

    def test_full_requires_every_candidate(self):
        j = sample()
        j['options']['products_per_category'] = 0
        self.assertTrue(validate_job(j, 'full')['passed'])
        j['candidates'].append({'category': 'iphone', 'url': 'missing', 'specs_url': None})
        self.assertFalse(validate_job(j, 'full')['passed'])

    def test_wrong_mode_and_terminal_failure(self):
        self.assertFalse(validate_job(sample(), 'full')['passed'])
        j = sample()
        j['status'] = 'failed'
        self.assertFalse(validate_job(j, 'sample')['passed'])


if __name__ == '__main__':
    unittest.main()
