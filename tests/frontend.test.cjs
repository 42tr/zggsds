const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('frontend/app.js', 'utf8');

function page(user, handleRequest = () => []) {
    const nodes = new Map();
    const errors = [];
    const requests = [];
    const node = key => {
        if (!nodes.has(key)) nodes.set(key, {
            value: '', style: {}, options: [], textContent: '', innerHTML: '',
            addEventListener() {},
        });
        return nodes.get(key);
    };
    const context = vm.createContext({
        URLSearchParams,
        localStorage: { getItem: () => user },
        window: { location: { href: '/' }, addEventListener() {} },
        document: {
            addEventListener() {}, getElementById: node, querySelector: node,
            querySelectorAll: () => [],
            createElement: () => ({ textContent: '', get innerHTML() { return this.textContent; } }),
        },
        alert: message => errors.push(message),
        fetch: async (url, options) => {
            requests.push(url);
            assert.equal(options.credentials, 'same-origin');
            const data = handleRequest(url);
            return { status: 200, ok: true, json: async () => data };
        },
    });
    vm.runInContext(source, context);
    return { context, node, errors, requests };
}

for (const role of ['employee', 'project_manager']) {
    test(`${role} loads time entries without account administration API`, async () => {
        const p = page(JSON.stringify({ user_id: 7, username: 'alice', role }), url => {
            assert.notEqual(url, '/users');
            if (url === '/users/options') return [{ id: 7, username: 'alice' }];
            if (url === '/projects') return [];
            return { entries: [{ id: 1, user_id: 7, work_date: '2026-09-09', hours: 2, description: 'work', status: 'pending' }], total: 1 };
        });
        await vm.runInContext('loadTimeEntries()', p.context);
        assert.deepEqual(p.errors, []);
        assert.match(p.node('#time-entries-table tbody').innerHTML, /alice/);
        assert.match(p.node('#time-entries-table tbody').innerHTML, /work/);
    });
}

test('broken or missing user cache redirects to login', () => {
    for (const value of ['null', '{invalid', '{}']) {
        assert.equal(page(value).context.window.location.href, 'login.html');
    }
});

test('HTTP failure rejects rather than proceeding as a successful mutation', async () => {
    const p = page('{}');
    p.context.fetch = async () => ({ status: 403, ok: false, text: async () => '无权限' });
    await assert.rejects(vm.runInContext("fetchWithAuth('/users', { method: 'POST' })", p.context), /无权限/);
});

test('dashboard totals include records beyond the server page size limit', async () => {
    const p = page(JSON.stringify({ user_id: 1, username: 'admin', role: 'admin' }), url => {
        if (!url.startsWith('/time-entries?')) return [];
        const page = new URL(url, 'http://test').searchParams.get('page');
        return { entries: Array.from({ length: page === '1' ? 100 : 1 }, () => ({ hours: 1, status: 'approved' })), total_pages: 2 };
    });
    await vm.runInContext('loadDashboard()', p.context);
    assert.deepEqual(p.errors, []);
    assert.equal(p.node('total-hours').textContent, '101.0');
});
