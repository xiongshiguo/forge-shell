// 熔炉 Web UI — Claude Code 风格 V2.2
var currentMode = 'interactive';
var currentModelPref = 'auto';
var activeAbortController = null;
var isStreaming = false;
var currentRightPanel = 'cost';
var rightPanelLocked = false; // 用户手动切换后锁定，当前会话不再自动切换
var toolCallCount = 0;
var monthlyCost = 0.0;

// === 启动 ===
document.addEventListener('DOMContentLoaded', async function() {
  try {
    var resp = await fetch('/api/check-key');
    var data = await resp.json();
    data.has_key ? showMainUI() : showSetup();
  } catch(e) { showSetup(); }
});

// === 配置页 ===
function showSetup() {
  document.getElementById('setup-page').style.display = 'flex';
  document.getElementById('app').style.display = 'none';
  var btn = document.getElementById('setup-btn');
  var inp = document.getElementById('setup-key');
  var msg = document.getElementById('setup-msg');
  async function submit() {
    var key = inp.value.trim();
    if (!key) { msg.textContent = '请输入 API Key'; msg.style.color = 'var(--red)'; return; }
    if (!key.startsWith('sk-')) { msg.textContent = 'Key 应以 sk- 开头'; msg.style.color = 'var(--red)'; return; }
    btn.disabled = true; btn.textContent = '保存中…';
    var r = await fetch('/api/setup', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({api_key:key}) });
    var d = await r.json();
    if (d.success) { msg.textContent = d.message; msg.style.color = 'var(--green)'; setTimeout(showMainUI, 600); }
    else { msg.textContent = d.message; msg.style.color = 'var(--red)'; btn.disabled = false; btn.textContent = '保存并开始使用'; }
  }
  btn.addEventListener('click', submit);
  inp.addEventListener('keydown', function(e) { if(e.key==='Enter') submit(); });
}

// === 主界面 ===
function showMainUI() {
  document.getElementById('setup-page').style.display = 'none';
  document.getElementById('app').style.display = 'flex';
  setupModes();
  setupSend();
  setupRightPanelTabs();
  syncVersion();
  checkUpdate();
  loadSettings();
  loadSessionsList();
  refreshRightPanel();
  setInterval(refreshStats, 10000);
  setInterval(refreshErrorBadge, 30000);
  setInterval(refreshRightPanel, 15000);
  loadLatestSession().then(function(loaded) {
    if (!loaded) addMsg('system', '熔炉已就绪');
  });
}

// === 模式 ===
function setupModes() {
  // 底部模式按钮
  document.querySelectorAll('.mode-btn').forEach(function(b) {
    b.addEventListener('click', function() {
      setMode(b.dataset.mode);
    });
  });
}

function setMode(mode) {
  currentMode = mode;
  document.querySelectorAll('.mode-btn').forEach(function(b) {
    b.classList.toggle('active', b.dataset.mode === mode);
  });
}

// === 设置面板 ===
function loadSettings() {
  try {
    var s = JSON.parse(localStorage.getItem('forge_settings') || '{}');
    document.getElementById('set-flash').checked = s.flash !== false;
    document.getElementById('set-pro').checked = s.pro !== false;
    document.getElementById('set-local').checked = s.local === true;
    document.getElementById('set-budget').value = s.budget || 50;
    document.getElementById('set-theme').value = s.theme || 'purple';
  } catch(e) {}
}

function saveSettings() {
  var s = {
    flash: document.getElementById('set-flash').checked,
    pro: document.getElementById('set-pro').checked,
    local: document.getElementById('set-local').checked,
    budget: parseInt(document.getElementById('set-budget').value) || 50,
    theme: document.getElementById('set-theme').value
  };
  localStorage.setItem('forge_settings', JSON.stringify(s));
  // 自动调整模型偏好
  if (!s.flash && !s.pro) {
    document.getElementById('set-flash').checked = true;
    return saveSettings();
  }
  if (!s.pro && currentModelPref === 'pro') currentModelPref = 'flash';
  if (!s.flash && currentModelPref === 'flash') currentModelPref = 'pro';
}

function toggleSettings() {
  var el = document.getElementById('settings-modal');
  el.style.display = el.style.display === 'flex' ? 'none' : 'flex';
}

// === 发送/输入 ===
function setupSend() {
  document.getElementById('send-btn').addEventListener('click', sendMessage);
  document.getElementById('user-input').addEventListener('keydown', function(e) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
  });
}

function sendMessage() {
  if (isStreaming) { stopGeneration(); return; }
  var inp = document.getElementById('user-input');
  var text = inp.value.trim();
  if (!text) return;
  inp.value = '';
  addMsg('user', text);
  streamChat(text);
}

function stopGeneration() {
  if (activeAbortController) { activeAbortController.abort(); activeAbortController = null; }
  var el = document.getElementById('streaming-msg');
  if (el) {
    var text = el.querySelector('.msg-content').textContent;
    if (text.trim()) {
      el.classList.remove('streaming');
      el.querySelector('.msg-content').innerHTML = renderMarkdown(text);
    } else { el.remove(); }
  }
  isStreaming = false;
  setSendButton(false);
}

function setSendButton(streaming) {
  var btn = document.getElementById('send-btn');
  isStreaming = streaming;
  if (streaming) {
    btn.textContent = '中断';
    btn.classList.add('stopping');
  } else {
    btn.textContent = '发送';
    btn.classList.remove('stopping');
  }
}

// === 核心：流式聊天 ===
async function streamChat(msg) {
  setSendButton(true);
  var streamMsg = createStreamingMsg();

  var controller = new AbortController();
  activeAbortController = controller;

  // thinking 内容也记录用于右侧面板
  var thinkingBuffer = '';

  try {
    var resp = await fetch('/api/chat', {
      method:'POST',
      headers:{'Content-Type':'application/json'},
      body:JSON.stringify({message:msg, mode:currentMode, model_pref:currentModelPref}),
      signal: controller.signal
    });
    var reader = resp.body.getReader();
    var decoder = new TextDecoder();
    var buffer = '';
    var fullContent = '';
    var lastSave = Date.now();

    while (true) {
      var r = await reader.read();
      if (r.done) break;
      buffer += decoder.decode(r.value, {stream:true});
      var lines = buffer.split('\n');
      buffer = lines.pop() || '';
      for (var i = 0; i < lines.length; i++) {
        if (lines[i].startsWith('data: ')) {
          try {
            var data = JSON.parse(lines[i].slice(6));
            if (data.type === 'thinking') thinkingBuffer += data.content;
            fullContent = handleSSE(data, streamMsg, fullContent);
          } catch(e) {}
        }
      }
      if (Date.now() - lastSave > 10000) {
        autoSaveSession();
        lastSave = Date.now();
      }
    }
  } catch(e) {
    if (e.name === 'AbortError') { autoSaveSession(); return; }
    var text = streamMsg.querySelector('.msg-content').textContent;
    if (text.trim()) {
      streamMsg.classList.remove('streaming');
      streamMsg.querySelector('.msg-content').innerHTML = renderMarkdown(text);
    } else {
      addMsg('error', '连接中断: ' + (e.message || 'network error'));
      streamMsg.remove();
    }
    autoSaveSession();
  } finally {
    activeAbortController = null;
    setSendButton(false);
    // 更新右侧思考面板为完成状态
    if (thinkingBuffer) {
      updateThinkingPanel(thinkingBuffer, true);
    }
  }
}

function createStreamingMsg() {
  var div = document.createElement('div');
  div.className = 'message assistant streaming';
  div.id = 'streaming-msg';
  div.innerHTML = '<details class="thinking-block" open><summary>思考中…</summary><div class="thinking-content"></div></details><div class="msg-content"></div>';
  document.getElementById('messages').appendChild(div);
  scrollDown();
  return div;
}

function finalizeStreamingMsg(streamMsg, content, hasThinking) {
  streamMsg.classList.remove('streaming');
  streamMsg.removeAttribute('id');
  if (hasThinking) {
    var det = streamMsg.querySelector('.thinking-block');
    if (det) {
      det.querySelector('summary').textContent = '已深度思考';
      det.open = false;
    }
  } else {
    var det = streamMsg.querySelector('.thinking-block');
    if (det) det.remove();
  }
  if (content) {
    streamMsg.querySelector('.msg-content').innerHTML = renderMarkdown(content);
  } else if (!streamMsg.querySelector('.msg-content').textContent.trim()) {
    streamMsg.remove();
  }
  scrollDown();
}

function handleSSE(data, streamMsg, fullContent) {
  var thinkingEl = streamMsg.querySelector('.thinking-content');
  var contentEl = streamMsg.querySelector('.msg-content');

  switch (data.type) {
    case 'meta':
      // 更新模型显示
      var modelBadge = document.getElementById('topbar-model');
      if (modelBadge) {
        var m = data.model || '';
        modelBadge.textContent = m.includes('flash') ? 'Flash' : m.includes('pro') ? 'Pro' : '自动';
      }
      // 首次收到meta时自动切换到费用面板
      if (!rightPanelLocked) switchRightPanel('cost');
      break;

    case 'thinking':
      if (thinkingEl) {
        thinkingEl.textContent += data.content;
        var det = streamMsg.querySelector('.thinking-block');
        if (det) det.querySelector('summary').textContent = '思考中… (' + thinkingEl.textContent.length + ' 字)';
      }
      // 自动切换到思考面板
      if (!rightPanelLocked) switchRightPanel('thinking');
      updateThinkingPanel(thinkingEl ? thinkingEl.textContent : '', false);
      scrollDown();
      break;

    case 'chunk':
      fullContent += data.content;
      contentEl.innerHTML = renderMarkdown(fullContent);
      // 开始输出正式回答，切换到费用面板
      if (!rightPanelLocked && fullContent.length < 50) switchRightPanel('cost');
      scrollDown();
      break;

    case 'tool_start':
      toolCallCount = 0;
      (data.tools || []).forEach(function(t) {
        toolCallCount++;
        var names = {web:'联网搜索',search:'代码搜索',read:'读取文件',exec:'执行命令',lsp:'LSP检查','auto-fix':'自动修复',edit:'编辑文件',snap:'快照',rollback:'回滚',save:'记忆保存'};
        var shortArg = (t.arg || '').length > 40 ? (t.arg || '').substring(0, 40) + '…' : (t.arg || '');
        addToolMsg(t.tool, names[t.tool] || t.tool, shortArg, 'running');
        addToolToPanel(t.tool, names[t.tool] || t.tool, shortArg, 'running');
      });
      // 自动切换到工具面板
      if (!rightPanelLocked) switchRightPanel('tools');
      break;

    case 'tool_result':
      updateToolStatus(data.tool, data.arg, data.success);
      updateToolInPanel(data.tool, data.arg, data.success);
      toolCallCount--;
      if (toolCallCount <= 0) {
        // 所有工具执行完毕，切回费用面板
        if (!rightPanelLocked) switchRightPanel('cost');
      }
      break;

    case 'error':
      addMsg('error', data.message);
      streamMsg.remove();
      autoSaveSession();
      break;

    case 'done':
      var hasThinking = thinkingEl && thinkingEl.textContent.trim().length > 0;
      finalizeStreamingMsg(streamMsg, fullContent, hasThinking);
      autoSaveSession();
      loadSessionsList();
      if (!rightPanelLocked) switchRightPanel('cost');
      break;
  }
  return fullContent;
}

// === 右侧面板：思考更新 ===
function updateThinkingPanel(text, done) {
  var el = document.getElementById('rp-thinking-content');
  if (!el) return;
  if (!text || !text.trim()) {
    el.innerHTML = '<div class="rp-empty">等待任务…</div>';
    return;
  }
  // 显示前 500 字摘要
  var preview = text.trim().substring(0, 500);
  var lines = preview.split('\n').filter(function(l) { return l.trim(); });
  el.innerHTML = lines.map(function(l) {
    return '<div class="thinking-plan-item">' + escapeHtml(l.substring(0, 80)) + '</div>';
  }).join('');
  if (done) {
    el.innerHTML += '<div style="color:var(--green);font-size:11px;margin-top:8px">✓ 思考完成</div>';
  }
}

// === 右侧面板：工具列表 ===
function addToolToPanel(tool, name, arg, status) {
  var list = document.getElementById('rp-tools-list');
  if (!list) return;
  // 清除空状态
  var empty = list.querySelector('.rp-empty');
  if (empty) empty.remove();
  var icon = status === 'running' ? '⏳' : '✓';
  var cls = status === 'running' ? 'running' : 'ok';
  var div = document.createElement('div');
  div.className = 'rp-tool-item ' + cls;
  div.id = 'rptool-' + tool + '-' + (arg || '').replace(/[^a-zA-Z0-9]/g, '_');
  div.innerHTML = '<span class="rp-tool-icon">' + icon + '</span><span class="rp-tool-name">' + escapeHtml(name) + '</span><span class="rp-tool-time">' + escapeHtml(arg) + '</span>';
  list.appendChild(div);
}

function updateToolInPanel(tool, arg, success) {
  var id = 'rptool-' + tool + '-' + (arg || '').replace(/[^a-zA-Z0-9]/g, '_');
  var el = document.getElementById(id);
  if (el) {
    el.className = 'rp-tool-item ' + (success ? 'ok' : 'fail');
    el.querySelector('.rp-tool-icon').textContent = success ? '✓' : '✗';
  }
}

// === 右侧面板切换 ===
function switchRightPanel(name, source) {
  currentRightPanel = name;
  document.querySelectorAll('.rp-tab').forEach(function(t) {
    t.classList.toggle('active', t.dataset.panel === name);
  });
  document.querySelectorAll('.rp-module').forEach(function(m) {
    m.classList.toggle('active', m.id === 'rp-' + name);
  });
  // 用户手动切换后锁定
  if (source === 'manual') {
    rightPanelLocked = true;
  }
}

function setupRightPanelTabs() {
  document.querySelectorAll('.rp-tab').forEach(function(tab) {
    tab.addEventListener('click', function() {
      switchRightPanel(tab.dataset.panel, 'manual');
    });
  });
}

// === 右侧面板数据刷新 ===
async function refreshRightPanel() {
  try {
    var s = await fetch('/api/status'); var sd = await s.json();
    // 费用
    document.getElementById('rp-model-name').textContent = (sd.model || '').includes('flash') ? 'Flash' : (sd.model || '').includes('pro') ? 'Pro' : '自动';
    document.getElementById('rp-session-cost').textContent = '¥' + (sd.cost || 0).toFixed(4);
    document.getElementById('rp-cache-hit').textContent = ((sd.hit_rate || 0) * 100).toFixed(1) + '%';
    // 预计节省（相比全用Pro）
    var saved = (sd.cost || 0) * 9; // Flash是Pro的1/10
    document.getElementById('rp-saved').textContent = '¥' + saved.toFixed(2);
    // 月度累计
    monthlyCost = Math.max(monthlyCost, sd.cost || 0);
    document.getElementById('rp-monthly-cost').textContent = '¥' + monthlyCost.toFixed(2);
    // 预算条
    var budget = parseInt(document.getElementById('set-budget').value) || 50;
    var pct = Math.min(100, (monthlyCost / budget) * 100);
    document.getElementById('rp-budget-fill').style.width = pct + '%';
    document.getElementById('rp-budget-pct').textContent = pct.toFixed(0) + '%';

    // 项目信息
    try {
      var p = await fetch('/api/project'); var pd = await p.json();
      if (pd.ok) {
        var pc = document.getElementById('rp-project-content');
        pc.innerHTML = '<div class="rp-stat"><span class="rp-label">项目</span><span class="rp-value">' + escapeHtml(pd.name || '-') + '</span></div>' +
          '<div class="rp-stat"><span class="rp-label">文件数</span><span class="rp-value">' + (pd.file_count || 0) + '</span></div>' +
          '<div class="rp-stat"><span class="rp-label">代码行</span><span class="rp-value">' + (pd.total_lines || 0).toLocaleString() + '</span></div>' +
          '<div class="rp-stat"><span class="rp-label">Rust文件</span><span class="rp-value">' + (pd.rust_files || 0) + '</span></div>' +
          '<div class="rp-stat"><span class="rp-label">测试文件</span><span class="rp-value">' + (pd.test_files || 0) + '</span></div>';
      }
    } catch(e) {}
  } catch(e) {}
}

// === 工具消息跟踪 ===
var toolMsgIndex = {};

function addToolMsg(tool, name, arg, status) {
  var key = tool + ':' + arg;
  var div = document.createElement('div');
  div.className = 'message system tool';
  div.id = 'tool-' + key.replace(/[^a-zA-Z0-9]/g, '_');
  var statusText = status === 'running' ? '…' : status === 'ok' ? '✓' : '✗';
  div.innerHTML = '<span>' + statusText + ' ' + name + '</span> <span style="color:var(--text-dim);font-size:11px">' + escapeHtml(arg) + '</span>';
  document.getElementById('messages').appendChild(div);
  toolMsgIndex[key] = div;
  scrollDown();
}

function updateToolStatus(tool, arg, success) {
  var key = tool + ':' + (arg || '');
  var el = document.getElementById('tool-' + key.replace(/[^a-zA-Z0-9]/g, '_'));
  if (el) {
    var icon = success ? '✓' : '✗';
    el.querySelector('span').textContent = icon + el.querySelector('span').textContent.substring(1);
    el.style.color = success ? 'var(--green)' : 'var(--red)';
    el.style.borderLeftColor = success ? 'var(--green)' : 'var(--red)';
  }
}

// === 消息 ===
function addMsg(role, text, isHtml) {
  if (!text) return;
  var div = document.createElement('div');
  div.className = 'message ' + role;
  if (role === 'assistant') {
    div.innerHTML = '<div class="msg-content">' + (isHtml ? text : renderMarkdown(text)) + '</div>';
  } else {
    div.innerHTML = isHtml ? text : renderMarkdown(text);
  }
  document.getElementById('messages').appendChild(div);
  scrollDown();
}

function scrollDown() {
  var el = document.getElementById('messages');
  el.scrollTop = el.scrollHeight;
}

// === Markdown 渲染 ===
function renderMarkdown(text) {
  if (!text) return '';
  var blocks = [];
  var html = text;

  // 1. 保护代码块
  html = html.replace(/```(\w*)\n([\s\S]*?)```/g, function(m, lang, code) {
    var id = blocks.length;
    blocks.push('<pre class="code-block"><code class="language-' + (lang||'') + '">' + escapeHtml(code.trimEnd()) + '</code></pre>');
    return '\x00BLOCK' + id + '\x00';
  });

  // 2. 保护行内代码
  html = html.replace(/`([^`]+)`/g, function(m, code) {
    var id = blocks.length;
    blocks.push('<code class="inline-code">' + escapeHtml(code) + '</code>');
    return '\x00BLOCK' + id + '\x00';
  });

  // 3. 标题
  html = html.replace(/^### (.+)$/gm, '<h3>$1</h3>');
  html = html.replace(/^## (.+)$/gm, '<h2>$1</h2>');
  html = html.replace(/^# (.+)$/gm, '<h1>$1</h1>');

  // 4. 粗体/斜体
  html = html.replace(/\*\*\*(.+?)\*\*\*/g, '<strong><em>$1</em></strong>');
  html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  html = html.replace(/\*(.+?)\*/g, '<em>$1</em>');

  // 5. 表格
  html = html.replace(/^\|(.+)\|$/gm, function(m) {
    var cells = m.split('|').filter(function(c) { return c.trim(); });
    if (cells.every(function(c) { return /^[-:]+$/.test(c.trim()); })) return '';
    return '<tr>' + cells.map(function(c) { return '<td>' + c.trim() + '</td>'; }).join('') + '</tr>';
  });
  html = html.replace(/(<tr>.*<\/tr>\s*)+/g, '<table class="md-table">$&</table>');

  // 6. 列表
  html = html.replace(/^(\d+)\. (.+)$/gm, '<li>$2</li>');
  html = html.replace(/^[-*] (.+)$/gm, '<li>$1</li>');
  html = html.replace(/((?:<li>.*<\/li>\s*)+)/g, '<ul class="md-list">$&</ul>');

  // 7. 水平线
  html = html.replace(/^---$/gm, '<hr>');

  // 8. 段落
  html = html.replace(/\n\n+/g, '</p><p>');
  html = '<p>' + html + '</p>';
  html = html.replace(/\n/g, '<br>');
  html = html.replace(/<p>\s*<\/p>/g, '');

  // 9. 恢复代码块
  for (var i = 0; i < blocks.length; i++) {
    html = html.replace('\x00BLOCK' + i + '\x00', blocks[i]);
  }

  return html;
}

function escapeHtml(str) {
  return str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

// === 左侧面板：会话历史 ===
var sessionsCache = [];
var currentSessionId = null;

async function newSession() {
  await autoSaveSession();
  document.getElementById('messages').innerHTML = '';
  toolMsgIndex = {};
  rightPanelLocked = false; // 新会话重置
  addMsg('system', '新会话已开始（上一会话已自动保存）');
  loadSessionsList();
}

function toggleLeftPanel() {
  var el = document.getElementById('left-panel');
  if (window.innerWidth < 768) {
    el.classList.toggle('mobile-show');
  } else {
    el.style.display = el.style.display === 'none' ? 'flex' : 'none';
  }
}

async function loadSessionsList() {
  try {
    var r = await fetch('/api/sessions'); var d = await r.json();
    var el = document.getElementById('sessions-list');
    if (d.ok && d.sessions && d.sessions.length) {
      sessionsCache = d.sessions;
      // 按时间分组
      var groups = {};
      var today = new Date().toISOString().slice(5, 10); // MM-DD
      d.sessions.forEach(function(s) {
        var date = (s.date || '').substring(0, 5); // MM-DD
        var group;
        if (date === today) group = '今天';
        else if (date === yesterday()) group = '昨天';
        else if (isThisWeek(date)) group = '本周';
        else group = '更早';
        if (!groups[group]) groups[group] = [];
        groups[group].push(s);
      });

      var html = '';
      var order = ['今天', '昨天', '本周', '更早'];
      order.forEach(function(g) {
        if (groups[g] && groups[g].length) {
          html += '<div class="session-group"><div class="session-group-label">' + g + '</div>';
          groups[g].slice(0, 20).forEach(function(s) {
            var preview = (s.preview || '').substring(0, 30);
            var isActive = currentSessionId === s.id;
            html += '<div class="session-item' + (isActive ? ' active' : '') + '" onclick="restoreSession(\'' + s.id + '\')">' +
              '<div class="session-item-preview">' + escapeHtml(preview || '(空)') + '</div>' +
              '<div class="session-item-meta"><span>' + (s.date||'') + ' · ' + (s.turns||0) + '轮</span>' +
              '<span class="session-del" onclick="event.stopPropagation();deleteSession(\'' + s.id + '\')" title="删除">×</span></div>' +
              '</div>';
          });
          html += '</div>';
        }
      });
      el.innerHTML = html || '<div style="color:var(--text-dim);font-size:12px;padding:12px">暂无历史</div>';
    } else {
      el.innerHTML = '<div style="color:var(--text-dim);font-size:12px;padding:12px">暂无历史</div>';
    }
  } catch(e) {}
}

function filterSessions() {
  var q = (document.getElementById('session-search').value || '').toLowerCase();
  if (!q) { loadSessionsList(); return; }
  var filtered = sessionsCache.filter(function(s) {
    return (s.preview || '').toLowerCase().includes(q);
  });
  var el = document.getElementById('sessions-list');
  el.innerHTML = filtered.length ? filtered.slice(0, 20).map(function(s) {
    var preview = (s.preview || '').substring(0, 40);
    return '<div class="session-item" onclick="restoreSession(\'' + s.id + '\')">' +
      '<div class="session-item-preview">' + escapeHtml(preview) + '</div>' +
      '<div class="session-item-meta"><span>' + (s.date||'') + ' · ' + (s.turns||0) + '轮</span>' +
      '<span class="session-del" onclick="event.stopPropagation();deleteSession(\'' + s.id + '\')">×</span></div></div>';
  }).join('') : '<div style="color:var(--text-dim);font-size:12px;padding:12px">无匹配结果</div>';
}

async function deleteSession(id) {
  await fetch('/api/session/delete', {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({id:id})});
  loadSessionsList();
}

function restoreSession(id) {
  var found = sessionsCache.find(function(s) { return s.id === id; });
  if (!found || !found.messages || !found.messages.length) return;
  currentSessionId = id;
  document.getElementById('messages').innerHTML = '';
  toolMsgIndex = {};
  rightPanelLocked = false;
  found.messages.forEach(function(m) {
    if (m.role && m.content) {
      var isHtml = m.content.trim().startsWith('<');
      addMsg(m.role, m.content, isHtml);
    }
  });
  addMsg('system', '已恢复会话 (' + (found.date||'') + '，' + found.messages.length + ' 条消息)');
  loadSessionsList();
  // 移动端关闭面板
  if (window.innerWidth < 768) {
    document.getElementById('left-panel').classList.remove('mobile-show');
  }
}

// === 日期工具 ===
function yesterday() {
  var d = new Date(); d.setDate(d.getDate() - 1);
  return d.toISOString().slice(5, 10);
}

function isThisWeek(dateStr) {
  if (!dateStr) return false;
  var now = new Date();
  var dayOfWeek = now.getDay(); if (dayOfWeek === 0) dayOfWeek = 7;
  var monday = new Date(now); monday.setDate(now.getDate() - dayOfWeek + 1);
  var parts = dateStr.split('-');
  if (parts.length < 2) return false;
  var target = new Date(now.getFullYear(), parseInt(parts[0]) - 1, parseInt(parts[1]));
  return target >= monday;
}

// === 自动保存会话 ===
async function autoSaveSession() {
  var msgs = [];
  document.querySelectorAll('#messages .message').forEach(function(m) {
    var role = 'system';
    if (m.classList.contains('user')) role = 'user';
    else if (m.classList.contains('assistant')) role = 'assistant';
    var contentEl = m.querySelector('.msg-content');
    var content = contentEl ? contentEl.innerHTML : m.innerHTML;
    msgs.push({role: role, content: content});
  });
  if (msgs.length === 0) return;
  var turns = msgs.filter(function(m) { return m.role === 'user'; }).length;
  var lastUser = msgs.filter(function(m) { return m.role === 'user'; }).pop();
  var preview = (lastUser ? lastUser.content : '').replace(/<[^>]+>/g, '').substring(0, 80);
  try {
    await fetch('/api/session/auto-save', {
      method:'POST',
      headers:{'Content-Type':'application/json'},
      body:JSON.stringify({turns:turns, preview:preview, messages:msgs})
    });
  } catch(e) {}
}

// === 加载最新会话 ===
async function loadLatestSession() {
  try {
    var r = await fetch('/api/session/latest');
    var d = await r.json();
    if (d.ok && d.session && d.session.messages && d.session.messages.length) {
      currentSessionId = d.session.id || null;
      var msgs = d.session.messages;
      for (var i = 0; i < msgs.length; i++) {
        addMsg(msgs[i].role, msgs[i].content, true);
      }
      addMsg('system', '已恢复上次会话 (' + (d.session.date || '') + '，' + msgs.length + ' 条消息)');
      return true;
    }
  } catch(e) {}
  return false;
}

// === 更新检查 ===
async function syncVersion() {
  try {
    var r = await fetch('/api/update-check');
    var d = await r.json();
    document.getElementById('topbar-version').textContent = 'v' + (d.current || '0.21.0');
  } catch(e) {}
}

async function checkUpdate() {
  try {
    var r = await fetch('/api/update-check');
    var d = await r.json();
    if (d.update_available) {
      document.getElementById('update-banner').style.display = 'flex';
      document.getElementById('update-text').textContent = 'v' + d.latest + ' 可用！当前 v' + d.current;
    }
  } catch(e) {}
}

async function doUpdate() {
  document.getElementById('update-text').textContent = '下载中…';
  var r = await fetch('/api/update-now', {method:'POST'});
  var d = await r.json();
  if (d.ok) { document.getElementById('update-text').textContent = d.message; }
  else { document.getElementById('update-text').textContent = '更新失败: ' + (d.error || '?'); }
}

// === 统计刷新 ===
async function refreshStats() {
  try {
    var s = await fetch('/api/status'); var sd = await s.json();
    var cost = (sd.cost || 0).toFixed(4);
    document.getElementById('topbar-cost').textContent = '¥' + cost;
    document.getElementById('bottombar-cost').textContent = '💰 ¥' + cost;
    // 更新模型badge
    var badge = document.getElementById('topbar-model');
    if (badge) {
      var m = sd.model || '';
      badge.textContent = m.includes('flash') ? 'Flash' : m.includes('pro') ? 'Pro' : '自动';
    }
  } catch(e) {}
}

// === 错误日志 ===
function toggleErrorPanel() {
  var el = document.getElementById('error-panel');
  el.style.display = el.style.display === 'block' ? 'none' : 'block';
  if (el.style.display === 'block') refreshErrorLogs();
}

async function refreshErrorBadge() {
  try {
    var r = await fetch('/api/logs'); var d = await r.json();
    if (d.ok) {
      var c = (d.logs||[]).length;
      var badge = document.getElementById('err-count-badge');
      if (c === 0) { badge.textContent = '✓'; badge.className = 'err-count-badge ok'; }
      else if (c <= 5) { badge.textContent = c; badge.className = 'err-count-badge warn'; }
      else { badge.textContent = c; badge.className = 'err-count-badge err'; }
    }
  } catch(e) {}
}

async function refreshErrorLogs() {
  try {
    var r = await fetch('/api/logs'); var d = await r.json();
    var listEl = document.getElementById('error-log-list');
    var diagEl = document.getElementById('diagnosis-box');
    if (d.ok) {
      var errs = d.logs || [];
      listEl.innerHTML = errs.length ? errs.reverse().map(function(e) {
        var icon = e.level === 'panic' ? 'EXPLODE' : e.level === 'warn' ? 'WARN' : 'ERR';
        return '<div class="err-entry"><div><span class="err-ts">' + e.timestamp + '</span> <span class="err-comp">[' + e.component + ']</span> ' + icon + ' ' + escapeHtml(e.message) + (e.count > 1 ? ' <span class="err-count-badge-num">x' + e.count + '</span>' : '') + '</div><div class="err-ctx">' + escapeHtml(e.context) + '</div></div>';
      }).join('') : '<div style="color:var(--green)">No errors</div>';
      if (d.diagnosis && d.diagnosis.length) {
        diagEl.style.display = 'block';
        diagEl.innerHTML = d.diagnosis.map(function(f) { return '<div>' + f + '</div>'; }).join('');
      } else diagEl.style.display = 'none';
    }
  } catch(e) {}
}

async function clearErrorLogs() {
  await fetch('/api/logs/clear', {method:'POST'});
  refreshErrorLogs();
}
