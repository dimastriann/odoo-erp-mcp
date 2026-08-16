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
            newInstance.value = item ? { ...item, mode: item.mode || 'inherit' } : { id: '', name: '', url: '', db: '', username: '', password: '', active: false, mode: 'inherit' };
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
        async function updateGlobalMode(default_mode) { await api('/api/global-settings', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ default_mode }) }); await fetchConfig(); }
        const getInstanceMode = inst => inst.mode && inst.mode !== 'inherit' ? inst.mode : (config.value.global_settings?.default_mode || 'crud');
        const displayMode = mode => mode === 'read_only' ? 'Read only' : 'Full access';
        function toggleDarkMode() { isDark.value = !isDark.value; localStorage.setItem('theme', isDark.value ? 'dark' : 'light'); document.documentElement.classList.toggle('light', !isDark.value); }

        onMounted(() => { document.documentElement.classList.toggle('light', !isDark.value); fetchVersion(); fetchConfig(); });
        return { isDark, authenticated, loginForm, loginError, formError, config, serverVersion, instanceSearch, activeCount, filteredInstances, showInstanceModal, showPromptModal, newInstance, newPrompt, login, logout, openInstanceModal, openPromptModal, saveInstance, savePrompt, deleteInstance, deletePrompt, toggleActive, updateGlobalMode, getInstanceMode, displayMode, toggleDarkMode };
    }
}).mount('#app');
