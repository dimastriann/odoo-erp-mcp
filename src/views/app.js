const { createApp, ref, computed, onMounted } = Vue;

createApp({
    setup() {
        const isDark = ref(localStorage.getItem('theme') !== 'light');
        const authenticated = ref(false);
        const loginForm = ref({ username: '', password: '' });
        const loginError = ref('');
        const formError = ref('');
        const config = ref({ global_settings: { default_mode: 'crud' }, instances: [], prompts: [] });
        const serverVersion = ref('—');
        const instanceSearch = ref('');
        const showInstanceModal = ref(false);
        const showPromptModal = ref(false);
        const newInstance = ref({});
        const newPrompt = ref({});
        const capabilities = ['read', 'create', 'update', 'delete', 'workflow', 'financial', 'admin'];

        const activeCount = computed(() => config.value.instances.filter(item => item.active).length);
        const filteredInstances = computed(() => {
            const query = instanceSearch.value.trim().toLowerCase();
            if (!query) return config.value.instances;
            return config.value.instances.filter(item => [item.name, item.url, item.db, item.username]
                .some(value => String(value || '').toLowerCase().includes(query)));
        });

        async function api(url, options = {}) {
            const response = await fetch(url, options);
            if (response.status === 401) authenticated.value = false;
            return response;
        }

        async function fetchConfig() {
            const response = await api('/api/config');
            if (!response.ok) return;
            config.value = await response.json();
            authenticated.value = true;
        }

        async function fetchVersion() {
            const response = await fetch('/api/version');
            if (response.ok) serverVersion.value = (await response.json()).version;
        }

        async function login() {
            loginError.value = '';
            const response = await api('/api/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(loginForm.value) });
            if (!response.ok) return void (loginError.value = 'Invalid username or password.');
            loginForm.value.password = '';
            await fetchConfig();
        }

        async function logout() {
            await api('/api/logout', { method: 'POST' });
            authenticated.value = false;
        }

        function openInstanceModal(item = null) {
            formError.value = '';
            const scopedPolicy = item?.permissions ? { models: item.permissions.models || {}, operations: item.permissions.operations || {} } : { models: {}, operations: {} };
            newInstance.value = item
                ? { ...item, mode: item.mode || 'inherit', credential_source: item.password_env ? 'environment' : 'inline', policy_enabled: !!item.permissions, capability_allow: [...(item.permissions?.allow || [])], capability_deny: [...(item.permissions?.deny || [])], scoped_policy_json: JSON.stringify(scopedPolicy, null, 2), confirm_empty_policy: false }
                : { id: '', name: '', url: '', db: '', username: '', password: '', password_env: '', active: false, mode: 'inherit', credential_source: 'environment', policy_enabled: false, capability_allow: [], capability_deny: [], scoped_policy_json: JSON.stringify(scopedPolicy, null, 2), confirm_empty_policy: false };
            showInstanceModal.value = true;
        }

        function openPromptModal(item = null) {
            formError.value = '';
            newPrompt.value = item ? { ...item } : { id: '', name: '', content: '' };
            showPromptModal.value = true;
        }

        async function saveInstance() {
            formError.value = '';
            const payload = { ...newInstance.value };
            delete payload.has_password;
            delete payload.credential_source;
            delete payload.policy_enabled;
            delete payload.capability_allow;
            delete payload.capability_deny;
            delete payload.scoped_policy_json;
            delete payload.confirm_empty_policy;
            if (newInstance.value.credential_source === 'environment') {
                if (!String(payload.password_env || '').trim()) return void (formError.value = 'Environment variable name is required.');
                payload.password = '';
                payload.password_env = payload.password_env.trim();
            } else {
                payload.password_env = null;
                if (!payload.password && (!newInstance.value.id || newInstance.value.password_env)) return void (formError.value = 'Enter a password or API key when switching to inline credentials.');
            }
            if (newInstance.value.policy_enabled) {
                let scopedPolicy;
                try { scopedPolicy = JSON.parse(newInstance.value.scoped_policy_json || '{}'); }
                catch (_) { return void (formError.value = 'Scoped policy must be valid JSON.'); }
                if (!scopedPolicy || Array.isArray(scopedPolicy) || typeof scopedPolicy !== 'object') return void (formError.value = 'Scoped policy must be a JSON object.');
                if (scopedPolicy.models && (Array.isArray(scopedPolicy.models) || typeof scopedPolicy.models !== 'object')) return void (formError.value = 'models must be a JSON object.');
                if (scopedPolicy.operations && (Array.isArray(scopedPolicy.operations) || typeof scopedPolicy.operations !== 'object')) return void (formError.value = 'operations must be a JSON object.');
                const conflicts = newInstance.value.capability_allow.filter(capability => newInstance.value.capability_deny.includes(capability));
                if (conflicts.length) return void (formError.value = `Capabilities cannot be both allowed and denied: ${conflicts.join(', ')}.`);
                const hasScopedRules = Object.keys(scopedPolicy.models || {}).length || Object.keys(scopedPolicy.operations || {}).length;
                const hasInstanceRules = newInstance.value.capability_allow.length || newInstance.value.capability_deny.length;
                if (!hasScopedRules && !hasInstanceRules && !newInstance.value.confirm_empty_policy) return void (formError.value = 'Confirm the empty policy before saving a deny-all configuration.');
                payload.permissions = { ...scopedPolicy, allow: newInstance.value.capability_allow, deny: newInstance.value.capability_deny };
            } else {
                payload.permissions = null;
            }
            const response = await api('/api/instances', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
            if (!response.ok) return void (formError.value = 'Could not save this instance. Check all required fields.');
            showInstanceModal.value = false;
            await fetchConfig();
        }

        async function savePrompt() {
            formError.value = '';
            const response = await api('/api/prompts', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(newPrompt.value) });
            if (!response.ok) return void (formError.value = 'Could not save this prompt.');
            showPromptModal.value = false;
            await fetchConfig();
        }

        async function remove(url, message) {
            if (!confirm(message)) return;
            const response = await api(url, { method: 'DELETE' });
            if (response.ok) await fetchConfig();
        }

        const deleteInstance = id => remove(`/api/instances/${id}`, 'Delete this Odoo instance?');
        const deletePrompt = id => remove(`/api/prompts/${id}`, 'Delete this prompt?');
        async function toggleActive(id) { await api(`/api/instances/${id}/active`, { method: 'POST' }); await fetchConfig(); }
        async function updateGlobalMode(default_mode) { await api('/api/global-settings', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ ...config.value.global_settings, default_mode }) }); await fetchConfig(); }
        const getInstanceMode = inst => inst.mode && inst.mode !== 'inherit' ? inst.mode : (config.value.global_settings?.default_mode || 'crud');
        const displayMode = mode => mode === 'read_only' ? 'Read only' : 'Full access';
        function toggleCapability(listName, capability) {
            const values = newInstance.value[listName];
            const index = values.indexOf(capability);
            if (index >= 0) values.splice(index, 1);
            else {
                values.push(capability);
                const otherName = listName === 'capability_allow' ? 'capability_deny' : 'capability_allow';
                const otherIndex = newInstance.value[otherName].indexOf(capability);
                if (otherIndex >= 0) newInstance.value[otherName].splice(otherIndex, 1);
            }
        }
        function toggleDarkMode() { isDark.value = !isDark.value; localStorage.setItem('theme', isDark.value ? 'dark' : 'light'); document.documentElement.classList.toggle('light', !isDark.value); }

        onMounted(() => { document.documentElement.classList.toggle('light', !isDark.value); fetchVersion(); fetchConfig(); });
        return { isDark, authenticated, loginForm, loginError, formError, config, serverVersion, instanceSearch, activeCount, filteredInstances, showInstanceModal, showPromptModal, newInstance, newPrompt, capabilities, login, logout, openInstanceModal, openPromptModal, saveInstance, savePrompt, deleteInstance, deletePrompt, toggleActive, updateGlobalMode, getInstanceMode, displayMode, toggleCapability, toggleDarkMode };
    }
}).mount('#app');
