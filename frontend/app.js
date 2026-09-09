const API_BASE = '';

let currentPage = 1;
let currentPageSize = 10;
let currentTimeEntries = [];

// approvals tabs
let currentApprovalTab = 'time';

const ROLE_MAP = {
    'employee': '员工',
    'dept_manager': '部门负责人',
    'project_manager': '项目负责人',
    'timekeeper': '工时管理员',
    'admin': '系统管理员',
    'manager': '经理',
};

const STATUS_MAP = {
    'pending': '待审批',
    'dept_approved': '部门已审批（待二审）',
    'approved': '已通过',
    'rejected': '已拒绝',
};

let currentEntryMode = 'single';
let activeProjectsCache = [];
let weekEntryRows = [];
let weekRowSeed = 1;
const WEEKDAY_LABELS = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];
const RECENT_PROJECTS_KEY = 'recent_time_entry_project_ids';
const MAX_RECENT_PROJECTS = 8;

function cleanMsg(msg) {
    return String(msg || '')
        .replace(/\r?\n/g, ' ')
        .replace(/\"/g, '')
        .trim();
}

function escapeHtml(value) {
    const div = document.createElement('div');
    div.textContent = value == null ? '' : String(value);
    return div.innerHTML;
}

function showError(err, prefix = '操作失败') {
    const msg = cleanMsg(err && err.message ? err.message : err);
    if (!msg || msg === 'Unauthorized') return;
    alert(prefix + ': ' + msg);
}

// 兜底：任何未捕获的接口异常，都要提示用户（避免“点了没反应”）
window.addEventListener('unhandledrejection', (e) => {
    showError(e.reason, '请求失败');
});


function getAuthHeaders() {
    return { 'Content-Type': 'application/json' };
}

async function fetchWithAuth(url, options = {}) {
    const opts = {
        ...options,
        headers: {
            ...(options.headers || {}),
            ...getAuthHeaders(),
        },
    };

    const resp = await fetch(url, { ...opts, credentials: 'same-origin' });
    if (resp.status === 401) {
        // token 失效/未登录：统一跳回登录页
        logout();
        throw new Error('Unauthorized');
    }
    return resp;
}

async function readErrorText(resp) {
    try {
        const text = await resp.text();
        return (text || '').replace(/\"/g, '').trim();
    } catch {
        return '';
    }
}

async function fetchJsonWithAuth(url, options = {}) {
    const resp = await fetchWithAuth(url, options);
    if (!resp.ok) {
        const text = await readErrorText(resp);
        throw new Error(text || `请求失败 (${resp.status})`);
    }
    return await resp.json();
}

function getCurrentUser() {
    return JSON.parse(localStorage.getItem('user') || '{}');
}

function openChangePasswordModal() {
    document.getElementById('cp-old-password').value = '';
    document.getElementById('cp-new-password').value = '';
    document.getElementById('cp-new-password2').value = '';
    document.getElementById('change-password-modal').style.display = 'flex';
}

function closeChangePasswordModal() {
    document.getElementById('change-password-modal').style.display = 'none';
}

async function changePassword() {
    const oldPwd = document.getElementById('cp-old-password').value;
    const newPwd = document.getElementById('cp-new-password').value;
    const newPwd2 = document.getElementById('cp-new-password2').value;

    if (newPwd.length < 6) {
        alert('新密码至少 6 位');
        return;
    }
    if (newPwd !== newPwd2) {
        alert('两次输入的新密码不一致');
        return;
    }

    const response = await fetchWithAuth(`${API_BASE}/auth/change-password`, {
        method: 'PUT',
        body: JSON.stringify({ old_password: oldPwd, new_password: newPwd })
    });

    const msg = await response.text();

    if (response.ok) {
        alert(msg.replace(/"/g, ''));
        // 修改密码后强制重新登录
        logout();
    } else {
        alert(('修改失败: ' + msg).replace(/"/g, ''));
    }
}

function canManageProjects() {
    const role = getCurrentUser().role;
    return role === 'admin' || role === 'timekeeper';
}

function canManageDepartments() {
    const role = getCurrentUser().role;
    return role === 'admin' || role === 'timekeeper';
}

function canApprove() {
    const role = getCurrentUser().role;
    return role === 'admin' || role === 'dept_manager' || role === 'project_manager';
}

function isWorkday(dateStr) {
    const d = new Date(dateStr);
    const day = d.getDay();
    return day !== 0 && day !== 6;
}

function toIsoDate(d) {
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

function parseDateLocal(dateStr) {
    const [y, m, d] = String(dateStr).split('-').map(Number);
    return new Date(y, (m || 1) - 1, d || 1);
}

function getMonday(date) {
    const d = new Date(date.getFullYear(), date.getMonth(), date.getDate());
    const day = d.getDay() || 7;
    d.setDate(d.getDate() - day + 1);
    return d;
}

function addDays(date, days) {
    const d = new Date(date.getFullYear(), date.getMonth(), date.getDate());
    d.setDate(d.getDate() + days);
    return d;
}

function getWeekDates(weekStartStr) {
    const monday = getMonday(parseDateLocal(weekStartStr));
    return Array.from({ length: 7 }, (_, i) => toIsoDate(addDays(monday, i)));
}

function getCurrentWeekStart() {
    return toIsoDate(getMonday(new Date()));
}

function checkAuth() {
    const token = localStorage.getItem('token');
    if (!token) {
        window.location.href = 'login.html';
        return;
    }
    const user = getCurrentUser();
    document.getElementById('current-user').textContent = `${user.username} (${ROLE_MAP[user.role] || user.role})`;

    const role = user.role;

    // 导航权限控制
    const isEmployee = role === 'employee';
    document.getElementById('nav-dashboard').style.display = isEmployee ? 'none' : 'block';
    document.getElementById('nav-time-entries').style.display = 'block';
    document.getElementById('nav-projects').style.display = isEmployee ? 'none' : 'block';
    // 部门管理：仅 admin/timekeeper
    document.getElementById('nav-departments').style.display = canManageDepartments() ? 'block' : 'none';
    // 用户管理：admin/timekeeper
    document.getElementById('nav-users').style.display = role === 'admin' ? 'block' : 'none';
    // 审批管理：admin/dept_manager/project_manager
    document.getElementById('nav-approvals').style.display = canApprove() ? 'block' : 'none';

    const firstVisible = Array.from(document.querySelectorAll('nav button')).find(btn => btn.style.display !== 'none');
    if (firstVisible) firstVisible.click();

    // 默认当前月份
    const now = new Date();
    const currentMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;
    const dashboardMonth = document.getElementById('dashboard-month');
    if (dashboardMonth && !dashboardMonth.value) dashboardMonth.value = currentMonth;
    const filterMonth = document.getElementById('filter-month');
    if (filterMonth && !filterMonth.value) filterMonth.value = currentMonth;

    const entryDate = document.getElementById('entry-date');
    if (entryDate && !entryDate.value) entryDate.value = toIsoDate(new Date());

    const weekStartInput = document.getElementById('week-start-date');
    if (weekStartInput && !weekStartInput.value) weekStartInput.value = getCurrentWeekStart();
}

function logout() {
    localStorage.removeItem('user');
    fetch(`${API_BASE}/auth/logout`, { method: 'POST', credentials: 'same-origin' })
        .finally(() => { window.location.href = 'login.html'; });
}

function openChangePasswordModal() {
    document.getElementById('cp-old-password').value = '';
    document.getElementById('cp-new-password').value = '';
    document.getElementById('cp-new-password2').value = '';
    document.getElementById('change-password-modal').style.display = 'flex';
}

function closeChangePasswordModal() {
    document.getElementById('change-password-modal').style.display = 'none';
}

async function changePassword() {
    const oldPassword = document.getElementById('cp-old-password').value;
    const newPassword = document.getElementById('cp-new-password').value;
    const newPassword2 = document.getElementById('cp-new-password2').value;

    if (newPassword.length < 6) {
        alert('新密码至少 6 位');
        return;
    }
    if (newPassword !== newPassword2) {
        alert('两次新密码不一致');
        return;
    }

    const resp = await fetchWithAuth(`${API_BASE}/auth/change-password`, {
        method: 'PUT',
        body: JSON.stringify({ old_password: oldPassword, new_password: newPassword })
    });

    const msg = await resp.text();
    if (resp.ok) {
        alert(msg.replace(/"/g, ''));
        // 修改密码后强制重新登录更安全
        logout();
    } else {
        alert('修改失败: ' + msg.replace(/"/g, ''));
    }
}

function showSection(sectionId) {
    document.querySelectorAll('section').forEach(s => s.classList.remove('active'));
    document.querySelectorAll('nav button').forEach(b => b.classList.remove('active'));

    const button = Array.from(document.querySelectorAll('nav button')).find(
        b => b.onclick && b.onclick.toString().includes(sectionId)
    );
    if (button) button.classList.add('active');

    document.getElementById(sectionId).classList.add('active');

    if (sectionId === 'dashboard') loadDashboard();
    if (sectionId === 'time-entries') {
        loadActiveProjects();
        loadTimeEntries();
        initWeekEntryPanel();
    }
    if (sectionId === 'projects') loadProjects();
    if (sectionId === 'users') {
        loadUsers();
        loadDepartments();
    }
    if (sectionId === 'departments') {
        loadDepartments();
        loadDepartmentTree();
    }
    if (sectionId === 'approvals') {
        // default tab
        switchApprovalTab(currentApprovalTab || 'time');
    }
}

// ===================== 首页 =====================

async function loadDashboard() {
    try {
        const month = document.getElementById('dashboard-month').value;

    let timeUrl = `${API_BASE}/time-entries?page=1&page_size=9999`;
    if (month) timeUrl += `&month=${month}`;

    const [users, projects, timeEntriesResp] = await Promise.all([
        fetchJsonWithAuth(`${API_BASE}/users`),
        fetchJsonWithAuth(`${API_BASE}/projects`),
        fetchJsonWithAuth(timeUrl)
    ]);

    const timeEntries = Array.isArray(timeEntriesResp) ? timeEntriesResp : (timeEntriesResp.entries || []);

    document.getElementById('total-users').textContent = Array.isArray(users) ? users.length : 0;
    document.getElementById('total-projects').textContent = Array.isArray(projects) ? projects.length : 0;

    const totalHours = timeEntries.filter(e => e.status === 'approved').reduce((sum, entry) => sum + entry.hours, 0);
    document.getElementById('total-hours').textContent = totalHours.toFixed(1);

    const pending = timeEntries.filter(e => e.status === 'pending' || e.status === 'dept_approved').length;
    document.getElementById('pending-approvals').textContent = pending;

    } catch (e) {
        showError(e, '加载首页数据失败');
    }
}

function clearDashboardFilter() {
    const now = new Date();
    document.getElementById('dashboard-month').value = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;
    loadDashboard();
}

// ===================== 用户管理 =====================

async function loadUsers() {
    try {
        const [users, departments] = await Promise.all([
        fetchJsonWithAuth(`${API_BASE}/users`),
        fetchJsonWithAuth(`${API_BASE}/departments/flat`)
    ]);

    const deptMap = {};
    departments.forEach(d => { deptMap[d.id] = d.name; });

    const tbody = document.querySelector('#users-table tbody');
    tbody.innerHTML = users.map(user => `
        <tr>
            <td>${user.id}</td>
            <td>${escapeHtml(user.username)}</td>
            <td>${escapeHtml(user.email)}</td>
            <td>${escapeHtml(ROLE_MAP[user.role] || user.role)}</td>
            <td>${escapeHtml(user.department_id ? (deptMap[user.department_id] || '-') : '-')}</td>
            <td>
                <button onclick="openEditUserModal(${user.id})" style="padding:4px 8px;font-size:12px;background:#17a2b8;color:white;border:none;border-radius:3px;cursor:pointer;margin-right:5px;">编辑</button>
                <button onclick="deleteUser(${user.id})" style="padding:4px 8px;font-size:12px;background:#dc3545;color:white;border:none;border-radius:3px;cursor:pointer;">删除</button>
            </td>
        </tr>
    `).join('');

    const select = document.getElementById('entry-user-id');
    if (select) {
        select.innerHTML = users.map(u => `<option value="${u.id}">${escapeHtml(u.username)}</option>`).join('');
    }

    } catch (e) {
        showError(e, '加载用户失败');
    }
}

async function deleteUser(id) {
    if (confirm('确定要删除此用户吗？')) {
        const response = await fetchWithAuth(`${API_BASE}/users/${id}`, {
            method: 'DELETE'
        });
        if (response.ok) {
            loadUsers();
        } else {
            alert('删除失败');
        }
    }
}

async function openEditUserModal(userId) {
    const [users, depts] = await Promise.all([
        fetchJsonWithAuth(`${API_BASE}/users`),
        fetchJsonWithAuth(`${API_BASE}/departments/flat`)
    ]);

    const user = users.find(u => u.id === userId);
    if (!user) { alert('用户不存在'); return; }

    document.getElementById('edit-user-id').value = user.id;
    document.getElementById('edit-user-username').value = user.username;
    document.getElementById('edit-user-email').value = user.email;
    document.getElementById('edit-user-password').value = '';
    document.getElementById('edit-user-role').value = user.role;

    const deptSelect = document.getElementById('edit-user-department-id');
    deptSelect.innerHTML = '<option value="">无部门</option>' +
        depts.map(d => `<option value="${d.id}">${escapeHtml(d.name)}</option>`).join('');
    deptSelect.value = user.department_id || '';

    document.getElementById('edit-user-modal').style.display = 'flex';
}

function closeEditUserModal() {
    document.getElementById('edit-user-modal').style.display = 'none';
}

async function updateUser() {
    const userId = document.getElementById('edit-user-id').value;
    const data = {
        username: document.getElementById('edit-user-username').value,
        email: document.getElementById('edit-user-email').value,
        role: document.getElementById('edit-user-role').value,
        department_id: document.getElementById('edit-user-department-id').value
            ? parseInt(document.getElementById('edit-user-department-id').value) : null
    };

    const password = document.getElementById('edit-user-password').value;
    if (password) data.password = password;

    const response = await fetchWithAuth(`${API_BASE}/users/${userId}`, {
        method: 'PUT',
        body: JSON.stringify(data)
    });

    if (response.ok) {
        closeEditUserModal();
        loadUsers();
        alert('用户更新成功');
    } else {
        alert('用户更新失败');
    }
}

// ===================== 项目管理 =====================

function projectLabel(p) {
    const prefix = p.project_no || p.code || '';
    return prefix ? `${prefix} ${p.name}` : p.name;
}

function projectSearchText(p) {
    return [
        p.project_no,
        p.code,
        p.name,
        p.project_type,
        p.owner,
        p.description,
    ].filter(Boolean).join(' ').toLowerCase();
}

function getRecentProjectIds() {
    try {
        const ids = JSON.parse(localStorage.getItem(RECENT_PROJECTS_KEY) || '[]');
        return Array.isArray(ids) ? ids.map(String) : [];
    } catch (e) {
        return [];
    }
}

function rememberRecentProject(projectId) {
    if (!projectId) return;
    const id = String(projectId);
    const ids = [id, ...getRecentProjectIds().filter(item => item !== id)].slice(0, MAX_RECENT_PROJECTS);
    localStorage.setItem(RECENT_PROJECTS_KEY, JSON.stringify(ids));
}

function sortProjectsForEntry(projects) {
    const recentIds = getRecentProjectIds();
    const recentRank = new Map(recentIds.map((id, idx) => [id, idx]));
    return [...(Array.isArray(projects) ? projects : [])].sort((a, b) => {
        const aRank = recentRank.has(String(a.id)) ? recentRank.get(String(a.id)) : Number.MAX_SAFE_INTEGER;
        const bRank = recentRank.has(String(b.id)) ? recentRank.get(String(b.id)) : Number.MAX_SAFE_INTEGER;
        if (aRank !== bRank) return aRank - bRank;
        return projectLabel(a).localeCompare(projectLabel(b), 'zh-Hans-CN');
    });
}

function ensureProjectPicker(selectId, pickerId, options = {}) {
    const select = document.getElementById(selectId);
    const picker = document.getElementById(pickerId);
    if (!select || !picker || picker.dataset.bound === '1') return;

    picker.dataset.bound = '1';
    picker.innerHTML = `
        <div class="project-picker-control" tabindex="0">
            <span class="project-picker-value">请选择项目</span>
            <span class="project-picker-arrow">⌄</span>
        </div>
        <div class="project-picker-menu">
            <input type="search" class="project-picker-search" placeholder="搜索项目编号、名称、负责人">
            <div class="project-picker-options"></div>
        </div>
    `;

    const control = picker.querySelector('.project-picker-control');
    const search = picker.querySelector('.project-picker-search');
    control.addEventListener('click', () => openProjectPicker(selectId, pickerId));
    control.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            openProjectPicker(selectId, pickerId);
        }
    });
    search.addEventListener('input', () => renderProjectPickerOptions(selectId, pickerId));
    search.addEventListener('keydown', (e) => {
        if (e.key === 'Escape') closeProjectPickers();
    });

    picker.dataset.placeholder = options.placeholder || '请选择项目';
}

function updateProjectPickerDisplay(selectId, pickerId) {
    const select = document.getElementById(selectId);
    const picker = document.getElementById(pickerId);
    if (!select || !picker) return;
    const value = picker.querySelector('.project-picker-value');
    const selected = activeProjectsCache.find(p => String(p.id) === String(select.value));
    if (value) value.textContent = selected ? projectLabel(selected) : (picker.dataset.placeholder || '请选择项目');
}

function openProjectPicker(selectId, pickerId) {
    const picker = document.getElementById(pickerId);
    if (!picker) return;
    closeProjectPickers(picker);
    picker.classList.add('open');
    positionProjectPickerMenu(picker);
    const search = picker.querySelector('.project-picker-search');
    if (search) {
        search.value = '';
        setTimeout(() => search.focus(), 0);
    }
    renderProjectPickerOptions(selectId, pickerId);
}

function closeProjectPickers(except = null) {
    document.querySelectorAll('.project-picker.open').forEach(picker => {
        if (picker !== except) {
            picker.classList.remove('open');
            resetProjectPickerMenuPosition(picker);
        }
    });
}

function resetProjectPickerMenuPosition(picker) {
    const menu = picker ? picker.querySelector('.project-picker-menu') : null;
    if (!menu) return;
    const list = menu.querySelector('.project-picker-options');
    menu.style.left = '';
    menu.style.right = '';
    menu.style.top = '';
    menu.style.bottom = '';
    menu.style.width = '';
    menu.style.maxHeight = '';
    if (list) list.style.maxHeight = '';
}

function positionProjectPickerMenu(picker) {
    const control = picker ? picker.querySelector('.project-picker-control') : null;
    const menu = picker ? picker.querySelector('.project-picker-menu') : null;
    if (!control || !menu) return;

    const rect = control.getBoundingClientRect();
    const viewportPadding = 12;
    const preferredWidth = picker.classList.contains('compact')
        ? Math.max(rect.width, 360)
        : rect.width;
    const width = Math.min(preferredWidth, window.innerWidth - viewportPadding * 2);
    const left = Math.min(
        Math.max(rect.left, viewportPadding),
        window.innerWidth - width - viewportPadding
    );
    const belowSpace = window.innerHeight - rect.bottom - viewportPadding;
    const aboveSpace = rect.top - viewportPadding;
    const openUpward = belowSpace < 220 && aboveSpace > belowSpace;
    const maxHeight = Math.max(160, Math.min(320, (openUpward ? aboveSpace : belowSpace) - 8));
    const search = menu.querySelector('.project-picker-search');
    const list = menu.querySelector('.project-picker-options');
    const searchHeight = search ? search.offsetHeight : 0;
    const listMaxHeight = Math.max(96, maxHeight - searchHeight - 24);

    menu.style.left = `${left}px`;
    menu.style.right = 'auto';
    menu.style.width = `${width}px`;
    menu.style.maxHeight = `${maxHeight}px`;
    menu.style.top = openUpward ? 'auto' : `${rect.bottom + 4}px`;
    menu.style.bottom = openUpward ? `${window.innerHeight - rect.top + 4}px` : 'auto';
    if (list) list.style.maxHeight = `${listMaxHeight}px`;
}

function renderProjectPickerOptions(selectId, pickerId) {
    const select = document.getElementById(selectId);
    const picker = document.getElementById(pickerId);
    if (!select || !picker) return;

    const search = picker.querySelector('.project-picker-search');
    const list = picker.querySelector('.project-picker-options');
    const keyword = (search && search.value ? search.value : '').trim().toLowerCase();
    const recentIds = new Set(getRecentProjectIds());
    const projects = sortProjectsForEntry(activeProjectsCache)
        .filter(p => !keyword || projectSearchText(p).includes(keyword))
        .slice(0, 80);

    if (!list) return;
    if (projects.length === 0) {
        list.innerHTML = '<div class="project-picker-empty">未找到匹配项目</div>';
        return;
    }

    list.innerHTML = projects.map(p => {
        const selected = String(p.id) === String(select.value);
        const recent = recentIds.has(String(p.id));
        const meta = [p.project_type, p.owner].filter(Boolean).join(' · ');
        return `
            <button type="button" class="project-picker-option${selected ? ' selected' : ''}" data-project-id="${p.id}">
                <span class="project-picker-option-main">${escapeHtml(projectLabel(p))}</span>
                <span class="project-picker-option-meta">${recent ? '最近使用' : escapeHtml(meta || '进行中')}</span>
            </button>
        `;
    }).join('');

    list.querySelectorAll('.project-picker-option').forEach(btn => {
        btn.addEventListener('click', () => {
            select.value = btn.dataset.projectId || '';
            select.dispatchEvent(new Event('change', { bubbles: true }));
            updateProjectPickerDisplay(selectId, pickerId);
            closeProjectPickers();
        });
    });
}

function hydrateProjectSelect(selectId, selected = '') {
    const select = document.getElementById(selectId);
    if (!select) return;
    const selectedStr = selected ? String(selected) : String(select.value || '');
    select.innerHTML = '<option value="" disabled>请选择项目</option>' +
        sortProjectsForEntry(activeProjectsCache)
            .map(p => `<option value="${p.id}">${escapeHtml(projectLabel(p))}</option>`)
            .join('');
    if (selectedStr) select.value = selectedStr;
}

document.addEventListener('click', (e) => {
    if (!e.target.closest('.project-picker')) closeProjectPickers();
});

window.addEventListener('resize', () => closeProjectPickers());
window.addEventListener('scroll', () => {
    document.querySelectorAll('.project-picker.open').forEach(positionProjectPickerMenu);
}, true);

async function loadProjects() {
    try {
        const [projResp, usersResp] = await Promise.all([
        fetchWithAuth(`${API_BASE}/projects`),
        fetchWithAuth(`${API_BASE}/users`)
    ]);
    const projects = await projResp.json();
    const users = await usersResp.json();

    const canEdit = canManageProjects();

    // 填充新增表单的负责人下拉
    const ownerSelect = document.getElementById('project-owner');
    if (ownerSelect) {
        ownerSelect.innerHTML = '<option value="">请选择</option>' +
            users.map(u => `<option value="${escapeHtml(u.username)}">${escapeHtml(u.username)}</option>`).join('');
    }

    // 显示/隐藏新增表单和操作列
    document.getElementById('project-form-wrap').style.display = canEdit ? 'block' : 'none';
    document.getElementById('projects-action-col').style.display = canEdit ? '' : 'none';

    const tbody = document.querySelector('#projects-table tbody');
    tbody.innerHTML = projects.map(p => `
        <tr>
            <td>${escapeHtml(p.project_no || '-')}</td>
            <td>${escapeHtml(p.name)}</td>
            <td>${escapeHtml(p.project_type)}</td>
            <td>${escapeHtml(p.status)}</td>
            <td>${escapeHtml(p.cycle || '-')}</td>
            <td>${escapeHtml(p.owner || '-')}</td>
            <td>${escapeHtml(p.description || '-')}</td>
            ${canEdit ? `<td>
                <button onclick="openEditProjectModal(${p.id})" style="padding:4px 8px;font-size:12px;background:#17a2b8;color:white;border:none;border-radius:3px;cursor:pointer;margin-right:5px;">编辑</button>
                <button onclick="deleteProject(${p.id})" style="padding:4px 8px;font-size:12px;background:#dc3545;color:white;border:none;border-radius:3px;cursor:pointer;">删除</button>
            </td>` : ''}
        </tr>
    `).join('');

    } catch (e) {
        showError(e, '加载项目失败');
    }
}

async function loadActiveProjects() {
    const response = await fetchWithAuth(`${API_BASE}/projects/active`);
    const projects = await response.json();
    activeProjectsCache = Array.isArray(projects) ? projects : [];

    const select = document.getElementById('entry-project-id');
    if (select) {
        ensureProjectPicker('entry-project-id', 'entry-project-picker');
        hydrateProjectSelect('entry-project-id');
        updateProjectPickerDisplay('entry-project-id', 'entry-project-picker');
    }

    const filterSelect = document.getElementById('filter-project-id');
    if (filterSelect && filterSelect.options.length <= 1) {
        // 筛选框显示所有项目
        const allResp = await fetchWithAuth(`${API_BASE}/projects`);
        const allProjects = await allResp.json();
        filterSelect.innerHTML = '<option value="">全部</option>' +
            allProjects.map(p => `<option value="${p.id}">${escapeHtml(projectLabel(p))}</option>`).join('');
    }

    renderWeekEntryTable();
}

function switchEntryMode(mode) {
    currentEntryMode = mode === 'weekly' ? 'weekly' : 'single';
    const singleBtn = document.getElementById('entry-mode-single');
    const weeklyBtn = document.getElementById('entry-mode-weekly');
    const singlePanel = document.getElementById('single-entry-panel');
    const weeklyPanel = document.getElementById('weekly-entry-panel');

    if (singleBtn) singleBtn.classList.toggle('active', currentEntryMode === 'single');
    if (weeklyBtn) weeklyBtn.classList.toggle('active', currentEntryMode === 'weekly');
    if (singlePanel) singlePanel.classList.toggle('hidden', currentEntryMode !== 'single');
    if (weeklyPanel) weeklyPanel.classList.toggle('hidden', currentEntryMode !== 'weekly');
}

function makeWeekRow(seed = null) {
    return {
        id: seed ?? weekRowSeed++,
        project_id: '',
        description: '',
        hours: ['', '', '', '', '', '', ''],
    };
}

function getWeekStartValue() {
    const weekInput = document.getElementById('week-start-date');
    if (!weekInput) return getCurrentWeekStart();
    if (!weekInput.value) weekInput.value = getCurrentWeekStart();
    const monday = toIsoDate(getMonday(parseDateLocal(weekInput.value)));
    if (weekInput.value !== monday) weekInput.value = monday;
    return monday;
}

function updateWeekRangeTip() {
    const tip = document.getElementById('week-range-tip');
    if (!tip) return;
    const start = getWeekStartValue();
    const end = toIsoDate(addDays(parseDateLocal(start), 6));
    tip.textContent = `当前周：${start} 至 ${end}`;
}

function rowTotal(row) {
    return row.hours.reduce((sum, value) => sum + (parseFloat(value) || 0), 0);
}

function formatHour(value) {
    const n = Number(value || 0);
    if (!Number.isFinite(n)) return '0';
    return Math.abs(n - Math.round(n)) < 1e-9 ? String(Math.round(n)) : n.toFixed(1);
}

function escapeHtml(str) {
    return String(str || '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function projectOptionsHtml(selected) {
    const selectedStr = selected ? String(selected) : '';
    const options = ['<option value="">请选择项目</option>'];
    const found = activeProjectsCache.some(p => String(p.id) === selectedStr);
    options.push(...sortProjectsForEntry(activeProjectsCache).map(p => {
        const sid = String(p.id);
        const selectedAttr = sid === selectedStr ? ' selected' : '';
        return `<option value="${sid}"${selectedAttr}>${escapeHtml(projectLabel(p))}</option>`;
    }));
    if (selectedStr && !found) {
        options.push(`<option value="${selectedStr}" selected>项目不可用(${selectedStr})</option>`);
    }
    return options.join('');
}

function renderWeekEntryTable() {
    const tbody = document.querySelector('#week-entry-table tbody');
    if (!tbody) return;
    if (weekEntryRows.length === 0) weekEntryRows = [makeWeekRow()];

    tbody.innerHTML = weekEntryRows.map(row => `
        <tr data-row-id="${row.id}">
            <td style="min-width: 220px;">
                <select id="week-project-id-${row.id}" class="native-project-select" onchange="handleWeekProjectChange(${row.id}, this.value)">
                    ${projectOptionsHtml(row.project_id)}
                </select>
                <div id="week-project-picker-${row.id}" class="project-picker compact"></div>
            </td>
            <td style="min-width: 220px;">
                <textarea oninput="handleWeekDescriptionChange(${row.id}, this.value)" placeholder="可填写工作内容">${escapeHtml(row.description)}</textarea>
            </td>
            ${row.hours.map((value, idx) => `
                <td class="day-cell">
                    <input
                        type="number"
                        min="0"
                        step="0.5"
                        value="${value === '' ? '' : escapeHtml(value)}"
                        oninput="handleWeekHoursChange(${row.id}, ${idx}, this.value)"
                    >
                </td>
            `).join('')}
            <td class="row-total" data-row-total="${row.id}">${formatHour(rowTotal(row))}</td>
            <td><button type="button" class="week-row-remove" onclick="removeWeekRow(${row.id})">删除</button></td>
        </tr>
    `).join('');

    weekEntryRows.forEach(row => {
        ensureProjectPicker(`week-project-id-${row.id}`, `week-project-picker-${row.id}`);
        updateProjectPickerDisplay(`week-project-id-${row.id}`, `week-project-picker-${row.id}`);
    });
    updateWeekTotals();
}

function updateWeekTotals() {
    const weekDates = getWeekDates(getWeekStartValue());
    const totals = [0, 0, 0, 0, 0, 0, 0];
    weekEntryRows.forEach(row => {
        row.hours.forEach((value, idx) => {
            totals[idx] += parseFloat(value) || 0;
        });
    });

    const warning = document.getElementById('week-warning');
    let hasError = false;
    const warnings = [];
    totals.forEach((total, idx) => {
        const td = document.querySelector(`#week-total-row td[data-day-index="${idx}"]`);
        if (!td) return;
        td.textContent = formatHour(total);
        const low = isWorkday(weekDates[idx]) && total + 1e-9 < 8.0;
        td.classList.toggle('low-total', low);
        if (low) {
            hasError = true;
            warnings.push(`${WEEKDAY_LABELS[idx]} ${weekDates[idx]} 合计 ${formatHour(total)}h（需>=8h）`);
        }
    });

    const weekTotalAll = document.getElementById('week-total-all');
    if (weekTotalAll) {
        weekTotalAll.textContent = formatHour(totals.reduce((sum, h) => sum + h, 0));
    }

    if (warning) {
        if (hasError) {
            warning.classList.add('error');
            warning.textContent = warnings.join('；');
        } else {
            warning.classList.remove('error');
            warning.textContent = '每日合计已满足工作日 >= 8 小时要求。';
        }
    }
    updateWeekRangeTip();
}

function addWeekRow(prefill = null) {
    const row = makeWeekRow();
    if (prefill) {
        row.project_id = prefill.project_id ? String(prefill.project_id) : '';
        row.description = prefill.description || '';
        row.hours = Array.isArray(prefill.hours) ? prefill.hours.slice(0, 7) : row.hours;
        while (row.hours.length < 7) row.hours.push('');
    }
    weekEntryRows.push(row);
    renderWeekEntryTable();
}

function removeWeekRow(rowId) {
    weekEntryRows = weekEntryRows.filter(row => row.id !== rowId);
    if (weekEntryRows.length === 0) weekEntryRows = [makeWeekRow()];
    renderWeekEntryTable();
}

function handleWeekProjectChange(rowId, value) {
    const row = weekEntryRows.find(r => r.id === rowId);
    if (!row) return;
    row.project_id = value;
    updateProjectPickerDisplay(`week-project-id-${rowId}`, `week-project-picker-${rowId}`);
}

function handleWeekDescriptionChange(rowId, value) {
    const row = weekEntryRows.find(r => r.id === rowId);
    if (!row) return;
    row.description = value;
}

function handleWeekHoursChange(rowId, dayIdx, value) {
    const row = weekEntryRows.find(r => r.id === rowId);
    if (!row) return;
    if (value === '') {
        row.hours[dayIdx] = '';
    } else {
        const parsed = parseFloat(value);
        row.hours[dayIdx] = Number.isFinite(parsed) && parsed >= 0 ? parsed : '';
    }
    updateWeekRowTotal(rowId);
    updateWeekTotals();
}

function updateWeekRowTotal(rowId) {
    const row = weekEntryRows.find(r => r.id === rowId);
    if (!row) return;
    const cell = document.querySelector(`#week-entry-table td[data-row-total="${rowId}"]`);
    if (cell) cell.textContent = formatHour(rowTotal(row));
}

function initWeekEntryPanel() {
    const weekInput = document.getElementById('week-start-date');
    if (!weekInput) return;
    if (!weekInput.dataset.bound) {
        weekInput.dataset.bound = '1';
        weekInput.addEventListener('change', () => {
            getWeekStartValue();
            weekEntryRows = [makeWeekRow()];
            renderWeekEntryTable();
            updateWeekTotals();
        });
    }
    getWeekStartValue();
    weekEntryRows = weekEntryRows.length > 0 ? weekEntryRows : [makeWeekRow()];
    renderWeekEntryTable();
    updateWeekTotals();
}

function buildRowsFromEntries(entries, weekDates) {
    const dateToIndex = {};
    weekDates.forEach((date, idx) => { dateToIndex[date] = idx; });
    const grouped = new Map();
    entries.forEach(entry => {
        const idx = dateToIndex[entry.work_date];
        if (idx === undefined) return;
        const key = `${entry.project_id || ''}::${entry.description || ''}`;
        if (!grouped.has(key)) {
            grouped.set(key, makeWeekRow());
        }
        const row = grouped.get(key);
        row.project_id = entry.project_id ? String(entry.project_id) : '';
        row.description = entry.description || '';
        const existing = parseFloat(row.hours[idx]) || 0;
        row.hours[idx] = existing + (parseFloat(entry.hours) || 0);
    });
    const rows = Array.from(grouped.values());
    return rows.length > 0 ? rows : [makeWeekRow()];
}

async function fetchUserWeekEntries(startDate, endDate) {
    const user = getCurrentUser();
    const params = new URLSearchParams({
        start_date: startDate,
        end_date: endDate,
        page: '1',
        page_size: '9999',
    });
    if (user && user.user_id) params.append('user_id', String(user.user_id));
    const data = await fetchJsonWithAuth(`${API_BASE}/time-entries?${params.toString()}`);
    return Array.isArray(data) ? data : (data.entries || []);
}

async function copyLastWeekToGrid() {
    const currentStart = getWeekStartValue();
    const lastStartDate = addDays(parseDateLocal(currentStart), -7);
    const lastStart = toIsoDate(lastStartDate);
    const lastEnd = toIsoDate(addDays(lastStartDate, 6));
    const lastEntries = await fetchUserWeekEntries(lastStart, lastEnd);
    weekEntryRows = buildRowsFromEntries(lastEntries, getWeekDates(lastStart));
    renderWeekEntryTable();
    const warning = document.getElementById('week-warning');
    if (warning) {
        warning.classList.remove('error');
        warning.textContent = lastEntries.length > 0 ? '已复制上周工时，可直接调整后提交' : '上周没有可复制的工时记录';
    }
}

function validateWeekBeforeSubmit() {
    const weekDates = getWeekDates(getWeekStartValue());
    const dayTotals = [0, 0, 0, 0, 0, 0, 0];
    const entries = [];

    for (const row of weekEntryRows) {
        const hasAnyHour = row.hours.some(value => (parseFloat(value) || 0) > 0);
        if (!hasAnyHour) continue;

        if (!row.project_id) {
            return { ok: false, message: '存在未选择项目的录入行' };
        }
        if (!row.description.trim()) {
            return { ok: false, message: '存在未填写描述的录入行' };
        }

        for (let i = 0; i < 7; i++) {
            const hours = parseFloat(row.hours[i]) || 0;
            if (hours <= 0) continue;
            if (!Number.isFinite(hours)) {
                return { ok: false, message: `${WEEKDAY_LABELS[i]} 工时格式不正确` };
            }
            if (Math.abs(hours * 2 - Math.round(hours * 2)) > 1e-9) {
                return { ok: false, message: `${WEEKDAY_LABELS[i]} 工时需按 0.5 递增` };
            }
            dayTotals[i] += hours;
            entries.push({
                project_id: parseInt(row.project_id, 10),
                work_date: weekDates[i],
                hours,
                description: row.description.trim(),
            });
        }
    }

    if (entries.length === 0) {
        return { ok: false, message: '请至少填写一条工时' };
    }

    for (let i = 0; i < 5; i++) {
        if (isWorkday(weekDates[i]) && dayTotals[i] + 1e-9 < 8.0) {
            return {
                ok: false,
                message: `${WEEKDAY_LABELS[i]} (${weekDates[i]}) 合计 ${formatHour(dayTotals[i])}h，不能低于8h`
            };
        }
    }
    return { ok: true, entries };
}

function entryUniqKey(entry) {
    return `${entry.work_date}|${entry.project_id || ''}|${String(entry.description || '').trim()}`;
}

async function submitWeekEntries() {
    const validation = validateWeekBeforeSubmit();
    if (!validation.ok) {
        alert(validation.message);
        return;
    }

    const weekStart = getWeekStartValue();
    const weekEnd = toIsoDate(addDays(parseDateLocal(weekStart), 6));
    const existingEntries = await fetchUserWeekEntries(weekStart, weekEnd);
    const existingKeys = new Set(
        existingEntries
            .filter(item => item.status !== 'rejected')
            .map(item => entryUniqKey(item))
    );
    const toSubmit = validation.entries.filter(item => !existingKeys.has(entryUniqKey(item)));
    if (toSubmit.length === 0) {
        alert('本周工时均已存在，无需重复提交');
        return;
    }

    const response = await fetchWithAuth(`${API_BASE}/time-entries/batch`, {
        method: 'POST',
        body: JSON.stringify({ entries: toSubmit })
    });
    if (!response.ok) {
        const err = await response.text();
        alert('批量提交失败: ' + err.replace(/"/g, ''));
        return;
    }

    const data = await response.json();
    toSubmit.forEach(item => rememberRecentProject(item.project_id));
    weekEntryRows = [makeWeekRow()];
    renderWeekEntryTable();
    loadTimeEntries();
    const count = data && typeof data.created_count === 'number' ? data.created_count : toSubmit.length;
    alert(`批量提交成功，共 ${count} 条`);
}

async function openEditProjectModal(projectId) {
    const [projResp, usersResp] = await Promise.all([
        fetchWithAuth(`${API_BASE}/projects`),
        fetchWithAuth(`${API_BASE}/users`)
    ]);
    const projects = await projResp.json();
    const users = await usersResp.json();
    const p = projects.find(x => x.id === projectId);
    if (!p) { alert('项目不存在'); return; }

    document.getElementById('edit-project-id').value = p.id;
    document.getElementById('edit-project-no').value = p.project_no || '';
    document.getElementById('edit-project-name').value = p.name;
    document.getElementById('edit-project-type').value = p.project_type;
    document.getElementById('edit-project-status').value = p.status;
    document.getElementById('edit-project-cycle').value = p.cycle || '';

    const ownerSelect = document.getElementById('edit-project-owner');
    ownerSelect.innerHTML = '<option value="">请选择</option>' +
        users.map(u => `<option value="${escapeHtml(u.username)}">${escapeHtml(u.username)}</option>`).join('');
    ownerSelect.value = p.owner || '';

    document.getElementById('edit-project-description').value = p.description || '';

    document.getElementById('edit-project-modal').style.display = 'flex';
}

function closeEditProjectModal() {
    document.getElementById('edit-project-modal').style.display = 'none';
}

async function updateProject() {
    const id = document.getElementById('edit-project-id').value;
    const data = {
        project_no: document.getElementById('edit-project-no').value || null,
        name: document.getElementById('edit-project-name').value,
        project_type: document.getElementById('edit-project-type').value,
        status: document.getElementById('edit-project-status').value,
        cycle: document.getElementById('edit-project-cycle').value || null,
        owner: document.getElementById('edit-project-owner').value || null,
        description: document.getElementById('edit-project-description').value || null,
    };

    const response = await fetchWithAuth(`${API_BASE}/projects/${id}`, {
        method: 'PUT',
        body: JSON.stringify(data)
    });

    if (response.ok) {
        closeEditProjectModal();
        loadProjects();
        alert('项目更新成功');
    } else {
        const err = await response.text();
        alert('更新失败: ' + err);
    }
}

async function deleteProject(id) {
    if (confirm('确定要删除此项目吗？')) {
        const response = await fetchWithAuth(`${API_BASE}/projects/${id}`, {
            method: 'DELETE'
        });
        if (response.ok) {
            loadProjects();
        } else {
            const err = await response.text();
            alert('删除失败: ' + err);
        }
    }
}

// ===================== 工时录入 =====================

async function loadTimeEntries() {
    try {
        const user = getCurrentUser();
    const startDate = document.getElementById('filter-start-date').value;
    const endDate = document.getElementById('filter-end-date').value;
    const userId = document.getElementById('filter-user-id').value;
    const projectId = document.getElementById('filter-project-id').value;
    const month = document.getElementById('filter-month').value;
    currentPageSize = parseInt(document.getElementById('page-size').value) || 10;

    let url = `${API_BASE}/time-entries?page=${currentPage}&page_size=${currentPageSize}`;
    if (startDate) url += `&start_date=${startDate}`;
    if (endDate) url += `&end_date=${endDate}`;
    if (userId) url += `&user_id=${userId}`;
    if (projectId) url += `&project_id=${projectId}`;
    if (month) url += `&month=${month}`;

    const [response, projectsResp, usersResp] = await Promise.all([
        fetchWithAuth(url),
        fetchWithAuth(`${API_BASE}/projects`),
        fetchWithAuth(`${API_BASE}/users`)
    ]);

    const data = await response.json();
    const projects = await projectsResp.json();
    const users = await usersResp.json();
    const entries = data.entries || data;
    currentTimeEntries = Array.isArray(entries) ? entries : [];

    const userSelect = document.getElementById('filter-user-id');
    if (userSelect.options.length <= 1) {
        userSelect.innerHTML = '<option value="">全部</option>' +
            users.map(u => `<option value="${u.id}">${escapeHtml(u.username)}</option>`).join('');
    }

    const projectSelect = document.getElementById('filter-project-id');
    if (projectSelect.options.length <= 1) {
        projectSelect.innerHTML = '<option value="">全部</option>' +
            projects.map(p => `<option value="${p.id}">${escapeHtml(projectLabel(p))}</option>`).join('');
    }

    const projectMap = {};
    const projectNoMap = {};
    const projectNameMap = {};
    projects.forEach(p => {
        projectMap[p.id] = projectLabel(p);
        projectNoMap[p.id] = p.project_no || p.code || '-';
        projectNameMap[p.id] = p.name || '-';
    });

    const tbody = document.querySelector('#time-entries-table tbody');
    tbody.innerHTML = entries.map(entry => {
        const entryUser = users.find(u => u.id === entry.user_id);
        const statusLabel = STATUS_MAP[entry.status] || entry.status;
        const statusClass = entry.status === 'approved' ? 'approved' :
            entry.status === 'rejected' ? 'rejected' : 'pending';

        const isOwn = entry.user_id === user.user_id;
        const canEdit = isOwn && (entry.status === 'pending' || entry.status === 'rejected' || entry.edit_allowed === 1);
        const canDelete = isOwn && entry.status === 'pending';
        const canRequestEdit = isOwn
            && entry.status !== 'pending'
            && entry.status !== 'rejected'
            && entry.edit_allowed !== 1
            && entry.edit_requested !== 1;

        let modLogCell = '-';
        if (entry.modification_log) {
            try {
                const logs = JSON.parse(entry.modification_log);
                if (logs.length > 0) {
                    const encodedLog = encodeURIComponent(JSON.stringify(logs)).replace(/'/g, '%27');
                    modLogCell = `<a href="javascript:void(0)" onclick="showModLogEncoded('${encodedLog}')" style="color:var(--primary);font-size:12px;">共${logs.length}次</a>`;
                }
            } catch(e) { /* ignore */ }
        }

        let buttons = '';
        if (canEdit) {
            buttons += `<button onclick="openEditTimeEntryModal(${entry.id})" style="padding:4px 8px;font-size:12px;background:#17a2b8;color:white;border:none;border-radius:3px;cursor:pointer;margin-right:4px;">编辑</button>`;
        }
        if (canDelete) {
            buttons += `<button onclick="deleteTimeEntry(${entry.id})" style="padding:4px 8px;font-size:12px;background:#dc3545;color:white;border:none;border-radius:3px;cursor:pointer;margin-right:4px;">撤回</button>`;
        }
        if (canRequestEdit) {
            buttons += `<button onclick="requestEdit(${entry.id})" style="padding:4px 8px;font-size:12px;background:#ffc107;color:#212529;border:none;border-radius:3px;cursor:pointer;">申请修改</button>`;
        }
        if (isOwn && entry.edit_requested === 1 && entry.status !== 'rejected') {
            buttons += `<span style="font-size:12px;color:#92400e;background:#fef3c7;padding:3px 8px;border-radius:3px;">已申请修改</span>`;
        }

        return `
            <tr>
                <td>${entry.work_date}</td>
                <td>${escapeHtml(entryUser ? entryUser.username : '-')}</td>
                <td>${escapeHtml(entry.project_id ? (projectNoMap[entry.project_id] || '-') : '-')}</td>
                <td>${escapeHtml(entry.project_id ? (projectNameMap[entry.project_id] || '-') : '-')}</td>
                <td>${entry.hours}</td>
                <td>${escapeHtml(entry.description)}</td>
                <td class="${statusClass}">${escapeHtml(statusLabel)}</td>
                <td>${modLogCell}</td>
                <td>${buttons}</td>
            </tr>
        `;
    }).join('');

    const total = data.total || entries.length;
    const totalPages = data.total_pages || 1;
    const page = data.page || 1;

    const paginationInfo = document.getElementById('pagination-info');
    if (paginationInfo) {
        paginationInfo.textContent = `第 ${page} 页，共 ${totalPages} 页，总计 ${total} 条记录`;
    }

    const prevBtn = document.getElementById('prev-page');
    const nextBtn = document.getElementById('next-page');
    if (prevBtn && nextBtn) {
        prevBtn.disabled = page <= 1;
        nextBtn.disabled = page >= totalPages;
    }

    } catch (e) {
        showError(e, '加载工时失败');
    }
}

function showModLogEncoded(encodedLog) {
    const logs = JSON.parse(decodeURIComponent(encodedLog));
    const lines = logs.map((log, i) => {
        const time = log.modified_at ? log.modified_at.replace('T', ' ').slice(0, 19) : '-';
        const before = log.before || {};
        const after = log.after || {};
        return `第${i + 1}次修改（${time}）\n`
            + `  日期: ${before.work_date || '-'} → ${after.work_date || '-'}\n`
            + `  工时: ${before.hours ?? '-'} → ${after.hours ?? '-'}\n`
            + `  描述: ${before.description || '-'} → ${after.description || '-'}`;
    }).join('\n\n');
    alert(lines || '暂无修改记录');
}

async function requestEdit(id) {
    const response = await fetchWithAuth(`${API_BASE}/time-entries/${id}/request-edit`, {
        method: 'PUT'
    });
    const msg = await response.text();
    alert(msg.replace(/"/g, ''));
}

async function openEditTimeEntryModal(entryId) {
    const entry = currentTimeEntries.find(e => e.id === entryId);
    if (!entry) { alert('工时记录不存在'); return; }

    const projects = activeProjectsCache.length > 0
        ? activeProjectsCache
        : await fetchJsonWithAuth(`${API_BASE}/projects/active`);
    activeProjectsCache = Array.isArray(projects) ? projects : [];

    document.getElementById('edit-entry-id').value = entry.id;
    document.getElementById('edit-entry-date').value = entry.work_date;
    document.getElementById('edit-entry-hours').value = entry.hours;
    document.getElementById('edit-entry-description').value = entry.description;

    const projectSelect = document.getElementById('edit-entry-project-id');
    ensureProjectPicker('edit-entry-project-id', 'edit-entry-project-picker');
    hydrateProjectSelect('edit-entry-project-id', entry.project_id || '');
    projectSelect.value = entry.project_id || '';
    updateProjectPickerDisplay('edit-entry-project-id', 'edit-entry-project-picker');

    document.getElementById('edit-time-entry-modal').style.display = 'flex';
}

function closeEditTimeEntryModal() {
    document.getElementById('edit-time-entry-modal').style.display = 'none';
}

async function updateTimeEntry() {
    const entryId = document.getElementById('edit-entry-id').value;
    const workDate = document.getElementById('edit-entry-date').value;
    const hours = parseFloat(document.getElementById('edit-entry-hours').value);
    const projectIdValue = document.getElementById('edit-entry-project-id').value;
    const description = document.getElementById('edit-entry-description').value.trim();

    if (!projectIdValue) {
        alert('请选择项目');
        return;
    }
    if (!Number.isFinite(hours) || hours <= 0) {
        alert('工时必须大于0');
        return;
    }
    if (!description) {
        alert('请填写描述');
        return;
    }

    const data = {
        project_id: parseInt(projectIdValue, 10),
        work_date: workDate,
        hours: hours,
        description
    };

    const response = await fetchWithAuth(`${API_BASE}/time-entries/${entryId}`, {
        method: 'PUT',
        body: JSON.stringify(data)
    });

    if (response.ok) {
        rememberRecentProject(projectIdValue);
        closeEditTimeEntryModal();
        loadTimeEntries();
        alert('工时更新成功');
    } else {
        const error = await response.text();
        alert('工时更新失败: ' + error.replace(/"/g, ''));
    }
}

async function deleteTimeEntry(id) {
    if (confirm('确定要撤回此工时记录吗？')) {
        const response = await fetchWithAuth(`${API_BASE}/time-entries/${id}`, {
            method: 'DELETE'
        });
        if (response.ok) {
            loadTimeEntries();
            alert('工时已撤回');
        } else {
            const error = await response.text();
            alert('撤回失败: ' + error.replace(/"/g, ''));
        }
    }
}

// ===================== 审批管理 =====================

function switchApprovalTab(tab) {
    currentApprovalTab = tab;

    const btnTime = document.getElementById('btn-approval-time');
    const btnReg = document.getElementById('btn-approval-register');
    if (btnTime && btnReg) {
        btnTime.classList.toggle('active', tab === 'time');
        btnReg.classList.toggle('active', tab === 'register');
    }

    const timeTable = document.getElementById('approvals-table');
    const regTable = document.getElementById('reg-approvals-table');

    if (timeTable) timeTable.style.display = (tab === 'time') ? '' : 'none';
    if (regTable) regTable.style.display = (tab === 'register') ? '' : 'none';

    if (tab === 'time') loadApprovalsTime();
    else loadApprovalsRegister();
}

async function loadApprovalsTime() {
    try {
        const user = getCurrentUser();
    const role = user.role;

    let stageText = '';
    if (role === 'dept_manager') stageText = '显示本部门待一审工时 / 作为项目负责人时的待二审工时';
    else if (role === 'project_manager') stageText = '显示待二审（交付/研发/售前）工时';
    else if (role === 'admin') stageText = '显示所有待审批工时';
    else if (role === 'timekeeper') stageText = '显示所有待审批工时';
    document.getElementById('approval-stage-tip').textContent = stageText;

    const [entriesResp, usersResp, projectsResp] = await Promise.all([
        fetchJsonWithAuth(`${API_BASE}/time-entries/pending`),
        fetchJsonWithAuth(`${API_BASE}/users`),
        fetchJsonWithAuth(`${API_BASE}/projects`)
    ]);

    const entries = Array.isArray(entriesResp) ? entriesResp : [];
    const users = Array.isArray(usersResp) ? usersResp : [];
    const projects = Array.isArray(projectsResp) ? projectsResp : [];

    const userMap = {};
    users.forEach(u => { userMap[u.id] = u.username; });
    const projectMap = {};
    projects.forEach(p => { projectMap[p.id] = projectLabel(p); });

    const canFirstApprove = role === 'admin' || role === 'dept_manager';
    const canSecondApprove = role === 'admin' || role === 'project_manager' || role === 'dept_manager';
    const canAllowEdit = role === 'admin' || role === 'dept_manager';

    const tbody = document.querySelector('#approvals-table tbody');
    tbody.innerHTML = entries.map(entry => {
        const statusLabel = STATUS_MAP[entry.status] || entry.status;
        const statusClass = entry.status === 'approved' ? 'approved' :
            entry.status === 'rejected' ? 'rejected' : 'pending';

        let buttons = '';
        if (entry.status === 'pending' && canFirstApprove) {
            buttons += `<button class="approve-btn" onclick="approveEntry(${entry.id}, 'approved')" style="margin-right:4px;">通过</button>`;
            buttons += `<button class="reject-btn" onclick="approveEntry(${entry.id}, 'rejected')">驳回</button>`;
        }
        if (entry.status === 'dept_approved' && canSecondApprove) {
            buttons += `<button class="approve-btn" onclick="secondApproveEntry(${entry.id}, 'approved')" style="margin-right:4px;">二审通过</button>`;
            buttons += `<button class="reject-btn" onclick="secondApproveEntry(${entry.id}, 'rejected')">二审驳回</button>`;
        }
        if (entry.edit_requested === 1 && canAllowEdit) {
            buttons += ` <button onclick="allowEdit(${entry.id})" style="padding:4px 8px;font-size:12px;background:#6610f2;color:white;border:none;border-radius:3px;cursor:pointer;margin-left:4px;">放开修改</button>`;
            buttons += ` <button onclick="denyEdit(${entry.id})" style="padding:4px 8px;font-size:12px;background:#dc3545;color:white;border:none;border-radius:3px;cursor:pointer;margin-left:4px;">拒绝修改</button>`;
        }

        const editRequestedTag = entry.edit_requested === 1
            ? ' <span style="font-size:11px;background:#fef3c7;color:#92400e;padding:1px 6px;border-radius:100px;">申请修改</span>'
            : '';

        return `
            <tr>
                <td>${entry.id}</td>
                <td>${escapeHtml(userMap[entry.user_id] || '-')}</td>
                <td>${escapeHtml(entry.project_id ? (projectMap[entry.project_id] || '-') : '-')}</td>
                <td>${entry.work_date}</td>
                <td>${entry.hours}</td>
                <td>${escapeHtml(entry.description)}</td>
                <td class="${statusClass}">${escapeHtml(statusLabel)}${editRequestedTag}</td>
                <td>${buttons}</td>
            </tr>
        `;
    }).join('');

    } catch (e) {
        showError(e, '加载待审批工时失败');
    }
}

async function loadApprovalsRegister() {
    try {
        const user = getCurrentUser();
    const role = user.role;

    if (!(role === 'admin' || role === 'timekeeper' || role === 'dept_manager')) {
        document.getElementById('approval-stage-tip').textContent = '无权限查看注册审批';
        const tbody = document.querySelector('#reg-approvals-table tbody');
        if (tbody) tbody.innerHTML = '';
        return;
    }

    document.getElementById('approval-stage-tip').textContent = '显示待审批注册申请（无部门负责人时会自动分配给 admin/timekeeper）';

    const [reqsResp, deptsResp] = await Promise.all([
        fetchJsonWithAuth(`${API_BASE}/approvals/registrations/pending`),
        fetchJsonWithAuth(`${API_BASE}/departments/flat`)
    ]);

    const reqs = Array.isArray(reqsResp) ? reqsResp : [];
    const depts = Array.isArray(deptsResp) ? deptsResp : [];
    const deptMap = {};
    depts.forEach(d => { deptMap[d.id] = d.name; });

    const tbody = document.querySelector('#reg-approvals-table tbody');
    tbody.innerHTML = reqs.map(r => {
        const assignee = (r.assigned_approver_role ? `${ROLE_MAP[r.assigned_approver_role] || r.assigned_approver_role}` : '-')
            + (r.assigned_approver_id ? `(#${r.assigned_approver_id})` : '');
        const deptLabel = deptMap[r.department_id] ? `${deptMap[r.department_id]}(${r.department_id})` : String(r.department_id);

        let buttons = '';
        // dept_manager 的权限在后端会再次校验
        buttons += `<button class="approve-btn" onclick="approveRegistration(${r.id})" style="margin-right:4px;">通过</button>`;
        buttons += `<button class="reject-btn" onclick="rejectRegistration(${r.id})">驳回</button>`;

        return `
          <tr>
            <td>${r.id}</td>
            <td>${escapeHtml(r.username)}</td>
            <td>${escapeHtml(r.email)}</td>
            <td>${escapeHtml(deptLabel)}</td>
            <td>${escapeHtml(assignee)}</td>
            <td>${escapeHtml(r.status)}</td>
            <td>${buttons}</td>
          </tr>
        `;
    }).join('');

    } catch (e) {
        showError(e, '加载注册审批失败');
    }
}

async function approveRegistration(id) {
    const resp = await fetchWithAuth(`${API_BASE}/approvals/registrations/${id}/approve`, {
        method: 'PUT'
    });
    const msg = await resp.text();
    if (resp.ok) {
        loadApprovalsRegister();
    } else {
        alert(('操作失败: ' + msg).replace(/"/g, ''));
    }
}

async function rejectRegistration(id) {
    const reason = prompt('请输入驳回原因：');
    if (reason === null) return;

    const resp = await fetchWithAuth(`${API_BASE}/approvals/registrations/${id}/reject`, {
        method: 'PUT',
        body: JSON.stringify({ reason })
    });
    const msg = await resp.text();
    if (resp.ok) {
        alert('已驳回');
        loadApprovalsRegister();
    } else {
        alert(('操作失败: ' + msg).replace(/"/g, ''));
    }
}

// 兼容旧入口：默认加载工时审批
async function loadApprovals() {
    switchApprovalTab(currentApprovalTab || 'time');
}

async function approveEntry(id, status) {
    const response = await fetchWithAuth(`${API_BASE}/time-entries/${id}/approve`, {
        method: 'PUT',
        body: JSON.stringify({ status })
    });
    const msg = await response.text();
    if (response.ok) {
        if (status !== 'approved') {
            alert(msg.replace(/"/g, ''));
        }
        loadApprovalsTime();
    } else {
        alert(('操作失败: ' + msg).replace(/"/g, ''));
    }
}

async function secondApproveEntry(id, status) {
    const response = await fetchWithAuth(`${API_BASE}/time-entries/${id}/second-approve`, {
        method: 'PUT',
        body: JSON.stringify({ status })
    });
    const msg = await response.text();
    if (response.ok) {
        if (status !== 'approved') {
            alert(msg.replace(/"/g, ''));
        }
        loadApprovalsTime();
    } else {
        alert(('操作失败: ' + msg).replace(/"/g, ''));
    }
}

async function allowEdit(id) {
    const response = await fetchWithAuth(`${API_BASE}/time-entries/${id}/allow-edit`, {
        method: 'PUT'
    });
    const msg = await response.text();
    alert(msg.replace(/"/g, ''));
    loadApprovalsTime();
}

async function denyEdit(id) {
    const response = await fetchWithAuth(`${API_BASE}/time-entries/${id}/deny-edit`, {
        method: 'PUT'
    });
    const msg = await response.text();
    alert(msg.replace(/"/g, ''));
    loadApprovalsTime();
}

// ===================== 部门管理 =====================

async function loadDepartments() {
    try {
        const response = await fetchWithAuth(`${API_BASE}/departments`);
    const departments = await response.json();

    const flattenDepts = (depts) => {
        let result = [];
        depts.forEach(d => {
            result.push(d);
            if (d.children && d.children.length > 0) {
                result = result.concat(flattenDepts(d.children));
            }
        });
        return result;
    };

    const flatDepts = flattenDepts(departments);

    const userDeptSelect = document.getElementById('user-department-id');
    if (userDeptSelect) {
        userDeptSelect.innerHTML = '<option value="">无部门</option>' +
            flatDepts.map(d => `<option value="${d.id}">${escapeHtml(d.name)}</option>`).join('');
    }

    const parentSelect = document.getElementById('dept-parent-id');
    if (parentSelect) {
        parentSelect.innerHTML = '<option value="">无上级部门</option>' +
            flatDepts.map(d => `<option value="${d.id}">${escapeHtml(d.name)}</option>`).join('');
    }

    } catch (e) {
        showError(e, '加载部门失败');
    }
}

async function loadDepartmentTree() {
    try {
        const canEdit = canManageDepartments();
    const response = await fetchWithAuth(`${API_BASE}/departments`);
    const departments = await response.json();

    // 仅 admin/timekeeper 显示表单和删除按钮
    const formWrap = document.getElementById('department-form-wrap');
    if (formWrap) formWrap.style.display = canEdit ? 'block' : 'none';

    function renderTree(depts, level = 0) {
        if (depts.length === 0) return '<p>暂无部门</p>';
        let html = '<ul>';
        depts.forEach(d => {
            const indent = 'padding-left: ' + (level * 20) + 'px;';
            html += `
                <li style="margin: 8px 0;">
                    <div style="display: flex; align-items: center; gap: 10px;">
                        <span style="${indent} font-weight: 500;">${escapeHtml(d.name)}</span>
                        ${canEdit ? `<button onclick="openEditDeptModal(${d.id})" style="padding:2px 8px;font-size:12px;background:#17a2b8;color:white;border:none;border-radius:3px;cursor:pointer;margin-right:4px;">编辑</button><button onclick="deleteDepartment(${d.id})" style="padding:2px 8px;font-size:12px;background:#dc3545;color:white;border:none;border-radius:3px;cursor:pointer;">删除</button>` : ''}
                    </div>
                    ${d.children && d.children.length > 0 ? renderTree(d.children, level + 1) : ''}
                </li>
            `;
        });
        html += '</ul>';
        return html;
    }

    document.getElementById('department-tree').innerHTML = renderTree(departments);

    } catch (e) {
        showError(e, '加载部门树失败');
    }
}

async function openEditDeptModal(deptId) {
    const response = await fetchWithAuth(`${API_BASE}/departments/flat`);
    const depts = await response.json();
    const d = depts.find(x => x.id === deptId);
    if (!d) { alert('部门不存在'); return; }

    document.getElementById('edit-dept-id').value = d.id;
    document.getElementById('edit-dept-name').value = d.name;

    const parentSelect = document.getElementById('edit-dept-parent-id');
    // 排除自身，避免选自己为上级
    parentSelect.innerHTML = '<option value="">无上级部门</option>' +
        depts.filter(x => x.id !== deptId).map(x => `<option value="${x.id}">${escapeHtml(x.name)}</option>`).join('');
    parentSelect.value = d.parent_id || '';

    document.getElementById('edit-dept-modal').style.display = 'flex';
}

function closeEditDeptModal() {
    document.getElementById('edit-dept-modal').style.display = 'none';
}

async function updateDepartment() {
    const id = document.getElementById('edit-dept-id').value;
    const data = {
        name: document.getElementById('edit-dept-name').value,
        parent_id: document.getElementById('edit-dept-parent-id').value
            ? parseInt(document.getElementById('edit-dept-parent-id').value) : null,
    };

    const response = await fetchWithAuth(`${API_BASE}/departments/${id}`, {
        method: 'PUT',
        body: JSON.stringify(data),
    });

    if (response.ok) {
        closeEditDeptModal();
        loadDepartments();
        loadDepartmentTree();
    } else {
        const err = await response.text();
        alert('更新失败: ' + err);
    }
}

async function deleteDepartment(id) {
    if (confirm('确定要删除此部门吗？')) {
        await fetchWithAuth(`${API_BASE}/departments/${id}`, {
            method: 'DELETE'
        });
        loadDepartmentTree();
        loadDepartments();
    }
}

// ===================== 表单提交 =====================

document.getElementById('user-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const data = {
        username: document.getElementById('user-username').value,
        email: document.getElementById('user-email').value,
        password: document.getElementById('user-password').value,
        role: document.getElementById('user-role').value,
        department_id: document.getElementById('user-department-id').value
            ? parseInt(document.getElementById('user-department-id').value) : null
    };

    await fetchWithAuth(`${API_BASE}/users`, {
        method: 'POST',
        body: JSON.stringify(data)
    });

    e.target.reset();
    loadUsers();
});

document.getElementById('project-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const data = {
        project_no: document.getElementById('project-no').value || null,
        name: document.getElementById('project-name').value,
        code: null,
        project_type: document.getElementById('project-type').value,
        status: document.getElementById('project-status').value,
        cycle: document.getElementById('project-cycle').value || null,
        owner: document.getElementById('project-owner').value || null,
        description: document.getElementById('project-description').value || null,
    };

    const response = await fetchWithAuth(`${API_BASE}/projects`, {
        method: 'POST',
        body: JSON.stringify(data)
    });

    if (response.ok) {
        e.target.reset();
        loadProjects();
    } else {
        const err = await response.text();
        alert('创建失败: ' + err.replace(/"/g, ''));
    }
});

document.getElementById('time-entry-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const workDate = document.getElementById('entry-date').value;
    const hours = parseFloat(document.getElementById('entry-hours').value);
    const projectIdValue = document.getElementById('entry-project-id').value;
    const description = document.getElementById('entry-description').value.trim();

    if (!projectIdValue) {
        alert('请选择项目');
        return;
    }
    if (!Number.isFinite(hours) || hours <= 0) {
        alert('工时必须大于0');
        return;
    }
    if (!description) {
        alert('请填写描述');
        return;
    }

    const data = {
        project_id: parseInt(projectIdValue, 10),
        work_date: workDate,
        hours: hours,
        description
    };

    const response = await fetchWithAuth(`${API_BASE}/time-entries`, {
        method: 'POST',
        body: JSON.stringify(data)
    });

    if (response.ok) {
        rememberRecentProject(projectIdValue);
        document.getElementById('entry-hours').value = '';
        document.getElementById('entry-date').value = toIsoDate(addDays(parseDateLocal(workDate), 1));
        loadTimeEntries();
    } else {
        const error = await response.text();
        alert('提交失败: ' + error.replace(/"/g, ''));
    }
});

document.getElementById('department-form')?.addEventListener('submit', async (e) => {
    e.preventDefault();
    const data = {
        name: document.getElementById('dept-name').value,
        parent_id: document.getElementById('dept-parent-id').value
            ? parseInt(document.getElementById('dept-parent-id').value) : null
    };

    await fetchWithAuth(`${API_BASE}/departments`, {
        method: 'POST',
        body: JSON.stringify(data)
    });

    e.target.reset();
    loadDepartments();
    loadDepartmentTree();
});

document.getElementById('edit-user-form')?.addEventListener('submit', async (e) => {
    e.preventDefault();
    await updateUser();
});

document.getElementById('edit-time-entry-form')?.addEventListener('submit', async (e) => {
    e.preventDefault();
    await updateTimeEntry();
});

document.getElementById('edit-project-form')?.addEventListener('submit', async (e) => {
    e.preventDefault();
    await updateProject();
});

document.getElementById('edit-dept-form')?.addEventListener('submit', async (e) => {
    e.preventDefault();
    await updateDepartment();
});

document.getElementById('change-password-form')?.addEventListener('submit', async (e) => {
    e.preventDefault();
    await changePassword();
});

// ===================== 分页 =====================

function prevPage() {
    if (currentPage > 1) {
        currentPage--;
        loadTimeEntries();
    }
}

function nextPage() {
    const totalText = document.getElementById('pagination-info').textContent;
    const match = totalText.match(/共 (\d+) 页/);
    if (match) {
        const totalPages = parseInt(match[1]);
        if (currentPage < totalPages) {
            currentPage++;
            loadTimeEntries();
        }
    }
}

function filterLastWeek() {
    const today = new Date();
    const dayOfWeek = today.getDay() || 7; // 周日=0 转为 7
    const lastMonday = new Date(today);
    lastMonday.setDate(today.getDate() - dayOfWeek - 6);
    const lastSunday = new Date(lastMonday);
    lastSunday.setDate(lastMonday.getDate() + 6);

    const fmt = d => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

    document.getElementById('filter-month').value = '';
    document.getElementById('filter-start-date').value = fmt(lastMonday);
    document.getElementById('filter-end-date').value = fmt(lastSunday);
    document.getElementById('filter-user-id').value = '';
    document.getElementById('filter-project-id').value = '';
    currentPage = 1;
    loadTimeEntries();
}

function clearTimeEntryFilter() {
    const now = new Date();
    const currentMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;
    document.getElementById('filter-user-id').value = '';
    document.getElementById('filter-project-id').value = '';
    document.getElementById('filter-start-date').value = '';
    document.getElementById('filter-end-date').value = '';
    document.getElementById('filter-month').value = currentMonth;
    document.getElementById('page-size').value = '10';
    currentPage = 1;
    loadTimeEntries();
}

async function exportTimeEntries() {
    const userId = document.getElementById('filter-user-id').value;
    const projectId = document.getElementById('filter-project-id').value;
    const startDate = document.getElementById('filter-start-date').value;
    const endDate = document.getElementById('filter-end-date').value;
    const month = document.getElementById('filter-month').value;

    const params = new URLSearchParams();
    if (userId) params.append('user_id', userId);
    if (projectId) params.append('project_id', projectId);
    if (startDate) params.append('start_date', startDate);
    if (endDate) params.append('end_date', endDate);
    if (month) params.append('month', month);

    let url = `${API_BASE}/time-entries/export`;
    if (params.toString()) url += `?${params.toString()}`;

    const response = await fetchWithAuth(url);

    if (response.ok) {
        const blob = await response.blob();
        const downloadUrl = window.URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = downloadUrl;
        a.download = `工时记录_${new Date().toISOString().slice(0, 10)}.xlsx`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        window.URL.revokeObjectURL(downloadUrl);
    } else {
        alert('导出失败');
    }
}

// ===================== 初始化 =====================

checkAuth();
