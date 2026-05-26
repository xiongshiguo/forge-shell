// 熔炉 Web UI — Claude Code 风格
var currentMode = 'assist';
var currentModelPref = 'auto';
var activeAbortController = null;
var isStreaming = false;

function setModelPref(val) { currentModelPref = val; }

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
  syncVersion();
  checkUpdate();
  setInterval(refreshStats, 10000);
  setInterval(refreshErrorBadge, 30000);
  loadLatestSession().then(function(loaded) {
    if (!loaded) addMsg('system', '熔炉已就绪');
  });
}

async function loadLatestSession() {
  try {
    var r = await fetch('/api/session/latest');
    var d = await r.json();
    if (d.ok && d.session && d.session.messages && d.session.messages.length) {
      var msgs = d.session.messages;
      for (var i = 0; i < msgs.length; i++) {
        addMsg(msgs[i].role, msgs[i].content);
      }
      addMsg('system', '已恢复上次会话 (' + (d.session.date || '') + '，' + msgs.length + ' 条消息)');
      return true;
    }
  } catch(e) {}
  return false;
}

async function syncVersion() {
  try {
    var r = await fetch('/api/update-check');
    var d = await r.json();
    document.getElementById('topbar-version').textContent = 'v' + (d.current || '0.21.0');
  } catch(e) {}
}

// === 模式切换 ===
function setupModes() {
  document.querySelectorAll('.mode-btn').forEach(function(b) {
    b.addEventListener('click', function() {
      document.querySelectorAll('.mode-btn').forEach(function(x) { x.classList.remove('active'); });
      b.classList.add('active');
      currentMode = b.dataset.mode;
    });
  });
}

// === 发送/输入 ===
function setupSend() {
  document.getElementById('send-btn').addEventListener('click', sendMessage);
  document.getElementById('user-input').addEventListener('keydown', function(e) { if(e.key==='Enter') sendMessage(); });
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
          try { fullContent = handleSSE(JSON.parse(lines[i].slice(6)), streamMsg, fullContent); } catch(e) {}
        }
      }
      // 每 10 秒增量保存（不等 done，崩溃也不丢数据）
      if (Date.now() - lastSave > 10000) {
        autoSaveSession();
        lastSave = Date.now();
      }
    }
  } catch(e) {
    if (e.name === 'AbortError') { autoSaveSession(); return; } // 用户中断也保存
    // 即使出错，保存已收到的部分内容
    var text = streamMsg.querySelector('.msg-content').textContent;
    if (text.trim()) {
      streamMsg.classList.remove('streaming');
      streamMsg.querySelector('.msg-content').innerHTML = renderMarkdown(text);
    } else {
      addMsg('error', '连接中断: ' + (e.message || 'network error'));
      streamMsg.remove();
    }
    autoSaveSession(); // 异常时保存当前进度
  } finally {
    activeAbortController = null;
    setSendButton(false);
  }
}

function createStreamingMsg() {
  var div = document.createElement('div');
  div.className = 'message assistant streaming';
  div.id = 'streaming-msg';
  // thinking 区块（默认折叠）+ 主内容区
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
      det.open = false; // 完成后折叠
    }
  } else {
    var det = streamMsg.querySelector('.thinking-block');
    if (det) det.remove(); // 无思考则移除区块
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
    case 'thinking':
      // 思考过程：流入可折叠区块
      if (thinkingEl) {
        thinkingEl.textContent += data.content;
        var det = streamMsg.querySelector('.thinking-block');
        if (det) det.querySelector('summary').textContent = '思考中… (' + thinkingEl.textContent.length + ' 字)';
      }
      scrollDown();
      break;

    case 'chunk':
      // 正式回答：流入主内容区
      fullContent += data.content;
      contentEl.innerHTML = renderMarkdown(fullContent);
      scrollDown();
      break;

    case 'tool_start':
      (data.tools || []).forEach(function(t) {
        var names = {web:'联网搜索',search:'代码搜索',read:'读取文件',exec:'执行命令',lsp:'LSP检查','auto-fix':'自动修复',edit:'编辑文件',snap:'快照',rollback:'回滚',save:'记忆保存'};
        var shortArg = (t.arg || '').length > 40 ? (t.arg || '').substring(0, 40) + '…' : (t.arg || '');
        addToolMsg(t.tool, names[t.tool] || t.tool, shortArg, 'running');
      });
      break;

    case 'tool_result':
      updateToolStatus(data.tool, data.arg, data.success);
      break;

    case 'error':
      addMsg('error', data.message);
      streamMsg.remove();
      autoSaveSession(); // 错误时保存进度
      break;

    case 'done':
      var hasThinking = thinkingEl && thinkingEl.textContent.trim().length > 0;
      finalizeStreamingMsg(streamMsg, fullContent, hasThinking);
      autoSaveSession();
      loadSessionsList();
      break;
  }
  return fullContent;
}

// 工具消息跟踪
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
function addMsg(role, text) {
  if (!text) return;
  var div = document.createElement('div');
  div.className = 'message ' + role;
  if (role === 'assistant') {
    div.innerHTML = '<div class="msg-content">' + renderMarkdown(text) + '</div>';
  } else {
    div.textContent = text;
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
  var html = text;
  // 代码块 ```...```
  html = html.replace(/```(\w*)\n([\s\S]*?)```/g, function(m, lang, code) {
    return '<pre class="code-block"><code>' + escapeHtml(code.trimEnd()) + '</code></pre>';
  });
  // 行内代码 `...`
  html = html.replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>');
  // 粗体 **...**
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  // 斜体 *...*
  html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');
  // 无序列表
  html = html.replace(/^- (.+)$/gm, '<li>$1</li>');
  html = html.replace(/(<li>.*<\/li>\n?)+/g, '<ul class="md-list">$&</ul>');
  // 表格
  html = html.replace(/^\|(.+)\|$/gm, function(m) {
    var cells = m.split('|').filter(function(c) { return c.trim(); });
    if (cells.every(function(c) { return /^[-:]+$/.test(c.trim()); })) return '';
    return '<tr>' + cells.map(function(c) { return '<td>' + c.trim() + '</td>'; }).join('') + '</tr>';
  });
  html = html.replace(/(<tr>.*<\/tr>\n?)+/g, '<table class="md-table">$&</table>');
  // 换行
  html = html.replace(/\n/g, '<br>');
  return html;
}

function escapeHtml(str) {
  return str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

// === 更新检查 ===
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
    document.getElementById('topbar-cost').textContent = '¥' + sd.cost.toFixed(4);
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

// === 自动保存会话（全量存储，无截断） ===
async function autoSaveSession() {
  var msgs = [];
  document.querySelectorAll('#messages .message').forEach(function(m) {
    var role = 'system';
    if (m.classList.contains('user')) role = 'user';
    else if (m.classList.contains('assistant')) role = 'assistant';
    msgs.push({role: role, content: m.textContent || ''});
  });
  if (msgs.length === 0) return;
  var turns = msgs.filter(function(m) { return m.role === 'user'; }).length;
  var lastUser = msgs.filter(function(m) { return m.role === 'user'; }).pop();
  var preview = (lastUser ? lastUser.content : '').substring(0, 80);
  try {
    await fetch('/api/session/auto-save', {
      method:'POST',
      headers:{'Content-Type':'application/json'},
      body:JSON.stringify({turns:turns, preview:preview, messages:msgs})
    });
  } catch(e) {}
}

// === 会话侧栏 ===
var sessionsCache = [];

async function newSession() {
  // 先保存当前会话
  await autoSaveSession();
  // 再清空并开始新会话
  document.getElementById('messages').innerHTML = '';
  toolMsgIndex = {};
  addMsg('system', '新会话已开始（上一会话已自动保存）');
  // 刷新会话列表
  loadSessionsList();
}

function toggleSessions() {
  var el = document.getElementById('sessions-panel');
  el.style.display = el.style.display === 'none' ? 'block' : 'none';
  if (el.style.display === 'block') loadSessionsList();
}

async function loadSessionsList() {
  try {
    var r = await fetch('/api/sessions'); var d = await r.json();
    var el = document.getElementById('sessions-list');
    if (d.ok && d.sessions && d.sessions.length) {
      sessionsCache = d.sessions;
      el.innerHTML = d.sessions.slice(0, 10).map(function(s) {
        var preview = (s.preview || '').substring(0, 40);
        return '<div class="session-item" onclick="restoreSession(\'' + s.id + '\')">' +
          '<div class="session-item-preview">' + escapeHtml(preview || '(空)') + '</div>' +
          '<div class="session-item-date">' + (s.date||'') + ' · ' + (s.turns||0) + '轮' +
          ' <span class="session-del" onclick="event.stopPropagation();deleteSession(\'' + s.id + '\')" title="删除">×</span></div>' +
          '</div>';
      }).join('');
    } else {
      el.innerHTML = '<div style="color:var(--text-dim);font-size:12px">暂无历史</div>';
    }
  } catch(e) {}
}

async function deleteSession(id) {
  await fetch('/api/session/delete', {method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({id:id})});
  loadSessionsList();
}

function restoreSession(id) {
  var found = sessionsCache.find(function(s) { return s.id === id; });
  if (!found || !found.messages || !found.messages.length) return;
  document.getElementById('messages').innerHTML = '';
  toolMsgIndex = {};
  found.messages.forEach(function(m) {
    if (m.role && m.content) addMsg(m.role, m.content);
  });
  addMsg('system', '已恢复会话 (' + (found.date||'') + '，' + found.messages.length + ' 条消息)');
  document.getElementById('sessions-panel').style.display = 'none';
}
