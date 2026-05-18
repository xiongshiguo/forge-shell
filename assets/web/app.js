// 熔炉 Web UI - 应用逻辑

let currentMode = 'assist';
const messagesEl = document.getElementById('messages');
const inputEl = document.getElementById('user-input');
const sendBtn = document.getElementById('send-btn');
const costEl = document.getElementById('topbar-cost');

// ---- 初始化 ----
document.addEventListener('DOMContentLoaded', () => {
  setupModeButtons();
  setupPanelTabs();
  setupSend();
  loadStatus();
  loadPanels();
  setInterval(loadStatus, 3000);
});

// ---- 模式切换 ----
function setupModeButtons() {
  document.querySelectorAll('.mode-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      document.querySelectorAll('.mode-btn').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      currentMode = btn.dataset.mode;
      addMessage('system', `已切换到「${btn.textContent}」模式`);
    });
  });
}

// ---- 面板切换 ----
function setupPanelTabs() {
  document.querySelectorAll('.panel-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.panel-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      document.querySelectorAll('.panel-content').forEach(p => p.classList.remove('active'));
      document.getElementById('panel-' + tab.dataset.panel).classList.add('active');
    });
  });
}

// ---- 发送消息 ----
function setupSend() {
  sendBtn.addEventListener('click', sendMessage);
  inputEl.addEventListener('keydown', e => {
    if (e.key === 'Enter') sendMessage();
  });
}

async function sendMessage() {
  const text = inputEl.value.trim();
  if (!text) return;
  inputEl.value = '';

  addMessage('user', text);
  const streamEl = document.getElementById('streaming');
  streamEl.textContent = '思考中…';

  try {
    const resp = await fetch('/api/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ message: text, mode: currentMode })
    });

    const reader = resp.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });

      const lines = buffer.split('\n');
      buffer = lines.pop() || '';

      for (const line of lines) {
        if (line.startsWith('data: ')) {
          try {
            const data = JSON.parse(line.slice(6));
            handleSSE(data);
          } catch (e) { /* skip parse errors */ }
        }
      }
    }
  } catch (err) {
    addMessage('system', '请求失败: ' + err.message);
  }
  streamEl.textContent = '';
}

function handleSSE(data) {
  if (data.type === 'plan') {
    addMessage('system', `拆解为 ${data.tasks} 个子任务，${data.groups} 组并行（增益 ${data.gain.toFixed(1)}x）`);
  } else if (data.type === 'chunk') {
    const streamEl = document.getElementById('streaming');
    streamEl.textContent += data.content;
  } else if (data.type === 'done') {
    const streamEl = document.getElementById('streaming');
    const text = streamEl.textContent;
    streamEl.textContent = '';
    if (text) addMessage('assistant', text);
    addMessage('system', `完成: ${data.success} 成功 / ${data.failure} 失败 | ${data.tokens} tokens | ${data.duration_ms}ms`);
  }
}

// ---- 消息渲染 ----
function addMessage(role, text) {
  const div = document.createElement('div');
  div.className = 'message ' + role;
  div.textContent = text;
  messagesEl.appendChild(div);
  messagesEl.scrollTop = messagesEl.scrollHeight;
}

// ---- 状态刷新 ----
async function loadStatus() {
  try {
    const resp = await fetch('/api/status');
    const data = await resp.json();
    costEl.textContent = `💰 ¥${data.cost.toFixed(4)} | ${(data.hit_rate * 100).toFixed(0)}%`;
  } catch (e) { /* 忽略 */ }
}

// ---- 面板数据 ----
async function loadPanels() {
  try {
    const [costResp, projResp] = await Promise.all([
      fetch('/api/cost'),
      fetch('/api/project')
    ]);
    const cost = await costResp.json();
    const proj = await projResp.json();

    document.getElementById('panel-cost').innerHTML = `
      <div style="margin-bottom:12px">
        <strong>累计费用</strong><br>
        <span style="font-size:24px;color:var(--gold)">¥${cost.total_cost.toFixed(4)}</span>
      </div>
      <div style="margin-bottom:12px">
        <strong>缓存命中率</strong><br>
        <span style="color:var(--green)">${(cost.cache_hit_rate * 100).toFixed(0)}%</span>
        &nbsp;节省 ¥${cost.cache_saved.toFixed(4)}
      </div>
      <div style="margin-bottom:12px">
        <strong>vs Claude Code</strong><br>
        <span style="color:var(--green)">节省 ${cost.vs_claude_savings_pct.toFixed(0)}%</span>
      </div>
      <div style="margin-bottom:12px">
        <strong>月度预算</strong><br>
        ¥${cost.monthly_used.toFixed(2)} / ¥${cost.monthly_budget}
      </div>
    `;

    document.getElementById('panel-project').innerHTML = `
      <div style="margin-bottom:12px">
        <strong>${proj.name}</strong><br>
        文件 ${proj.file_count} | 行数 ${proj.total_lines}
      </div>
      <div style="margin-bottom:12px">
        Rust: ${proj.rust_files} | 测试: ${proj.test_files}
      </div>
      <div style="margin-bottom:12px">
        <strong>最近提交</strong>
        ${proj.recent_commits.map(c => `
          <div style="font-size:12px;margin:4px 0;color:var(--text-dim)">
            <span style="color:var(--gold)">${c.hash}</span>
            ${c.message.substring(0, 40)}
            <br>${c.author} · ${c.date}
          </div>
        `).join('')}
      </div>
    `;
  } catch (e) { /* 忽略 */ }
}
