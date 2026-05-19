// 熔炉 Web UI

let currentMode = 'assist';

// ---- 启动：先检查 API Key ----
document.addEventListener('DOMContentLoaded', async () => {
  try {
    const resp = await fetch('/api/check-key');
    const data = await resp.json();
    if (data.has_key) {
      showMainUI();
    } else {
      showSetup();
    }
  } catch (e) {
    // 如果请求失败，显示 setup
    showSetup();
  }
});

// ---- 配置页 ----
function showSetup() {
  document.getElementById('setup-page').style.display = 'flex';
  document.getElementById('app').style.display = 'none';

  const btn = document.getElementById('setup-btn');
  const keyInput = document.getElementById('setup-key');
  const msgEl = document.getElementById('setup-msg');

  async function submitKey() {
    const key = keyInput.value.trim();
    if (!key) {
      msgEl.textContent = '请输入 API Key';
      msgEl.style.color = 'var(--red)';
      return;
    }
    if (!key.startsWith('sk-')) {
      msgEl.textContent = 'API Key 格式错误，应以 sk- 开头';
      msgEl.style.color = 'var(--red)';
      return;
    }

    btn.disabled = true;
    btn.textContent = '保存中…';
    msgEl.textContent = '';

    try {
      const resp = await fetch('/api/setup', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ api_key: key })
      });
      const data = await resp.json();

      if (data.success) {
        msgEl.textContent = data.message;
        msgEl.style.color = 'var(--green)';
        setTimeout(() => showMainUI(), 600);
      } else {
        msgEl.textContent = data.message;
        msgEl.style.color = 'var(--red)';
        btn.disabled = false;
        btn.textContent = '保存并开始使用';
      }
    } catch (e) {
      msgEl.textContent = '网络错误，请确认熔炉正在运行';
      msgEl.style.color = 'var(--red)';
      btn.disabled = false;
      btn.textContent = '保存并开始使用';
    }
  }

  btn.addEventListener('click', submitKey);
  keyInput.addEventListener('keydown', e => {
    if (e.key === 'Enter') submitKey();
  });
}

// ---- 主界面 ----
function showMainUI() {
  document.getElementById('setup-page').style.display = 'none';
  document.getElementById('app').style.display = 'flex';

  setupModeButtons();
  setupPanelTabs();
  setupSend();
  addMessage('system', '🔥 熔炉已就绪。输入指令开始编程。');
  checkUpdate();
  loadStatus();
  loadPanels();
  setInterval(loadStatus, 3000);
}

// ---- 版本检测 ----
async function checkUpdate() {
  try {
    const resp = await fetch('/api/update-check');
    const data = await resp.json();
    if (data.update_available) {
      addMessage('system', `🔔 新版本 v${data.latest} 可用！当前: v${data.current}。下载: ${data.download_url}`);
    }
  } catch(e) {}
}

// ---- 模式按钮 ----
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
  document.getElementById('send-btn').addEventListener('click', sendMessage);
  document.getElementById('user-input').addEventListener('keydown', e => {
    if (e.key === 'Enter') sendMessage();
  });
}

async function sendMessage() {
  const inputEl = document.getElementById('user-input');
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
            handleSSE(JSON.parse(line.slice(6)));
          } catch (e) {}
        }
      }
    }
  } catch (err) {
    addMessage('system', '请求失败: ' + err.message);
  }
  streamEl.textContent = '';
}

function handleSSE(data) {
  if (data.type === 'error') {
    document.getElementById('streaming').textContent = '';
    addMessage('system', '❌ ' + data.message);
  } else if (data.type === 'plan') {
    addMessage('system', '拆解为 ' + data.tasks + ' 个子任务，' + data.groups + ' 组并行（增益 ' + data.gain.toFixed(1) + 'x）');
  } else if (data.type === 'chunk') {
    document.getElementById('streaming').textContent += data.content;
  } else if (data.type === 'done') {
    var text = document.getElementById('streaming').textContent;
    document.getElementById('streaming').textContent = '';
    if (text) {
      addMessage('assistant', text);
      parseToolCalls(text);
    }
  }
}

// 解析 AI 回复中的工具调用
function parseToolCalls(text) {
  var lines = text.split('\n');
  for (var i = 0; i < lines.length; i++) {
    var line = lines[i].trim();
    if (line.startsWith('[TOOL:')) {
      var match = line.match(/\[TOOL:(\w+)(?::(.*))?\]/);
      if (match) {
        var tool = match[1];
        var arg = match[2] || '';
        executeTool(tool, arg);
      }
    }
  }
}

async function executeTool(tool, arg) {
  switch (tool) {
    case 'exec':
      addMessage('system', '🔧 执行: ' + arg + '...');
      try {
        var resp = await fetch('/api/exec', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({command: arg, cwd: '.'})
        });
        var data = await resp.json();
        if (data.ok) {
          addMessage('system', '✓ ' + arg + ' 通过\n' + (data.stdout || '').substring(0, 500));
        } else {
          addMessage('system', '✗ ' + arg + ' 失败\n' + (data.stderr || data.stdout || '').substring(0, 1000));
        }
      } catch(e) { addMessage('system', '执行异常: ' + e.message); }
      break;

    case 'auto-fix':
      addMessage('system', '🔧 启动自动修复循环...');
      var eventSource = new EventSource('/api/auto-fix');
      eventSource.onmessage = function(evt) {
        try {
          var d = JSON.parse(evt.data);
          if (d.type === 'chunk') addMessage('system', d.content);
          if (d.type === 'done') { addMessage('system', d.message); eventSource.close(); }
          if (d.type === 'error') { addMessage('system', '❌ ' + d.message); eventSource.close(); }
        } catch(e) {}
      };
      break;

    case 'rollback':
      try {
        var resp = await fetch('/api/rollback', {method: 'POST'});
        var data = await resp.json();
        addMessage('system', '已回滚 ' + data.rolled_back + ' 个文件');
      } catch(e) { addMessage('system', '回滚失败: ' + e.message); }
      break;

    case 'save':
      try {
        var resp = await fetch('/api/save-context', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({content: arg})
        });
        var data = await resp.json();
        addMessage('system', data.ok ? '已保存记忆' : '保存失败');
      } catch(e) { addMessage('system', '保存异常: ' + e.message); }
      break;

    case 'read':
      try {
        var resp = await fetch('/api/exec', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({command: 'type ' + arg, cwd: '.'})
        });
        var data = await resp.json();
        addMessage('system', '📄 ' + arg + ':\n' + (data.stdout || data.stderr || '').substring(0, 1000));
      } catch(e) {}
      break;
  }
}

// ---- 消息渲染 ----
function addMessage(role, text) {
  const div = document.createElement('div');
  div.className = 'message ' + role;
  div.textContent = text;
  const messagesEl = document.getElementById('messages');
  messagesEl.appendChild(div);
  messagesEl.scrollTop = messagesEl.scrollHeight;
}

// ---- 状态刷新 ----
async function loadStatus() {
  try {
    const resp = await fetch('/api/status');
    const data = await resp.json();
    document.getElementById('topbar-cost').textContent =
      `💰 ¥${data.cost.toFixed(4)} | ${(data.hit_rate * 100).toFixed(0)}%`;
  } catch (e) {}
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
  } catch (e) {}
}
