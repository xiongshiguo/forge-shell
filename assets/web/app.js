// 熔炉 Web UI

var currentMode = 'assist';

// 面板折叠
function toggleSection(id) {
  var el = document.getElementById(id);
  if (!el) return;
  var hdr = el.previousElementSibling;
  var arrow = hdr ? hdr.querySelector('.collapse-arrow') : null;
  if (el.style.display === 'none') { el.style.display = 'flex'; if (arrow) arrow.textContent = '▾'; }
  else { el.style.display = 'none'; if (arrow) arrow.textContent = '▸'; }
}

// 同步所有版本号
async function syncVersion() {
  try {
    var r = await fetch('/api/update-check');
    var d = await r.json();
    var v = d.current || '0.14.1';
    var badges = document.querySelectorAll('.version-badge, .version');
    badges.forEach(function(b) { if (b.classList.contains('version-badge') || b.classList.contains('version')) b.textContent = 'v' + v; });
  } catch(e) {}
}

document.addEventListener('DOMContentLoaded', async function() {
  try {
    var resp = await fetch('/api/check-key');
    var data = await resp.json();
    data.has_key ? showMainUI() : showSetup();
  } catch(e) { showSetup(); }
});

// ---- 配置页 ----
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

// ---- 主界面 ----
function showMainUI() {
  document.getElementById('setup-page').style.display = 'none';
  document.getElementById('app').style.display = 'flex';
  setupModes();
  setupSend();
  syncVersion();
  setupReview();
  loadFileTree();
  checkUpdate();
  refreshRight();
  setInterval(refreshRight, 5000);
  loadLatestSession();
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
      addMsg('system', '📂 已恢复上次会话 (' + (d.session.date || '') + '，' + msgs.length + ' 条消息)');
      return;
    }
  } catch(e) {}
  addMsg('system', '🔥 熔炉已就绪');
}

function setupModes() {
  document.querySelectorAll('.mode-btn').forEach(function(b) {
    b.addEventListener('click', function() {
      document.querySelectorAll('.mode-btn').forEach(function(x) { x.classList.remove('active'); });
      b.classList.add('active');
      currentMode = b.dataset.mode;
    });
  });
}

function setupSend() {
  document.getElementById('send-btn').addEventListener('click', sendMessage);
  document.getElementById('user-input').addEventListener('keydown', function(e) { if(e.key==='Enter') sendMessage(); });
}

// ---- 文件树 ----
async function loadFileTree() {
  try {
    var r = await fetch('/api/files');
    var d = await r.json();
    var el = document.getElementById('file-tree');
    el.innerHTML = '';
    if (d.ok) d.files.forEach(function(f) { el.appendChild(renderFileNode(f, '')); });
  } catch(e) { document.getElementById('file-tree').textContent = '加载失败'; }
}

function renderFileNode(node, path) {
  var div = document.createElement('div');
  div.className = 'file-item ' + (node.dir ? 'dir' : 'file ' + node.ext);
  div.textContent = node.dir ? '▸ ' + node.name : node.name;
  var fullPath = path ? path + '/' + node.name : node.name;

  if (node.dir) {
    var childrenDiv = document.createElement('div');
    childrenDiv.className = 'file-children';
    if (node.children) node.children.forEach(function(c) { childrenDiv.appendChild(renderFileNode(c, fullPath)); });
    div.addEventListener('click', function(e) { e.stopPropagation(); childrenDiv.classList.toggle('collapsed'); div.textContent = childrenDiv.classList.contains('collapsed') ? '▸ ' + node.name : '▾ ' + node.name; });
    div.appendChild(childrenDiv);
  } else {
    div.addEventListener('click', function(e) { e.stopPropagation(); executeTool('read', fullPath); });
  }
  return div;
}

// ---- 发送消息 ----
function sendMessage() {
  var inp = document.getElementById('user-input');
  var text = inp.value.trim();
  if (!text) return;
  inp.value = '';
  addMsg('user', text);
  streamChat(text);
}

// ---- 状态指示器 ----
var activeToolCards = {};

function setStatus(text) {
  document.getElementById('thinking-status').textContent = text;
}

async function streamChat(msg) {
  var streamEl = document.getElementById('streaming-content');
  var areaEl = document.getElementById('streaming-area');
  areaEl.style.display = 'block';
  streamEl.textContent = '';
  activeToolCards = {};
  setStatus('思考中…');

  try {
    var resp = await fetch('/api/chat', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({message:msg, mode:currentMode}) });
    var reader = resp.body.getReader();
    var decoder = new TextDecoder();
    var buffer = '';

    while (true) {
      var r = await reader.read();
      if (r.done) break;
      buffer += decoder.decode(r.value, {stream:true});
      var lines = buffer.split('\n');
      buffer = lines.pop() || '';
      for (var i = 0; i < lines.length; i++) {
        if (lines[i].startsWith('data: ')) {
          try { handleSSE(JSON.parse(lines[i].slice(6))); } catch(e) {}
        }
      }
    }
  } catch(e) { addMsg('error', '连接失败: ' + e.message); }
}

function handleSSE(data) {
  var streamEl = document.getElementById('streaming-content');
  switch (data.type) {
    case 'error':
      document.getElementById('streaming-area').style.display = 'none';
      addMsg('error', '❌ ' + data.message);
      break;

    case 'tool_start':
      setStatus('🔧 执行工具中…');
      // 为每个工具创建执行卡片
      (data.tools || []).forEach(function(t) {
        var card = createToolCard(t.tool, t.arg);
        streamEl.appendChild(card);
        activeToolCards[t.tool + ':' + t.arg] = card;
      });
      break;

    case 'tool_result':
      setStatus('📊 分析结果中…');
      var key = data.tool + ':' + (data.arg || '');
      var card = activeToolCards[key];
      if (card) {
        var statusEl = card.querySelector('.tool-card-status');
        var resultEl = card.querySelector('.tool-card-result');
        if (statusEl) {
          statusEl.textContent = data.success ? '✓ 完成' : '✗ 失败';
          statusEl.className = 'tool-card-status ' + (data.success ? 'ok' : 'fail');
        }
        if (resultEl && data.summary) {
          resultEl.textContent = data.summary;
          resultEl.style.display = 'block';
        }
      }
      break;

    case 'chunk':
      setStatus('💬 回复中…');
      streamEl.textContent += data.content;
      break;

    case 'done':
      setStatus('✓ 完成');
      document.getElementById('streaming-area').style.display = 'none';
      var text = streamEl.textContent;
      streamEl.textContent = '';
      if (text) {
        addMsg('assistant', text);
        // 后端已处理工具调用，前端只做兜底解析
        parseToolCallsFallback(text);
      }
      break;
  }
  document.getElementById('messages').scrollTop = document.getElementById('messages').scrollHeight;
}

function createToolCard(tool, arg) {
  var div = document.createElement('div');
  div.className = 'tool-card';
  var icons = {web:'🌐',search:'🔍',read:'📄',exec:'▶',lsp:'🔬','auto-fix':'🔧','lsp-rich':'🧬',edit:'✏',snap:'📸',rollback:'↩',save:'💾'};
  var names = {web:'联网搜索',search:'代码搜索',read:'读取文件',exec:'执行命令',lsp:'代码检查','auto-fix':'自动修复','lsp-rich':'深度分析',edit:'代码编辑',snap:'快照',rollback:'回滚',save:'记忆保存'};
  var shortArg = arg.length > 50 ? arg.substring(0, 50) + '…' : arg;
  div.innerHTML = '<span class="tool-card-icon">' + (icons[tool] || '🔧') + '</span>' +
    '<span class="tool-card-name">' + (names[tool] || tool) + '</span>' +
    '<span class="tool-card-arg">' + shortArg + '</span>' +
    '<span class="tool-card-status running">⏳ 执行中</span>' +
    '<div class="tool-card-result" style="display:none"></div>';
  return div;
}

// 兜底工具解析（后端已处理，此处仅做兼容）
function parseToolCallsFallback(text) {
  var lines = text.split('\n');
  for (var i = 0; i < lines.length; i++) {
    var line = lines[i].trim();
    if (line.startsWith('[TOOL:')) {
      var m = line.match(/\[TOOL:(\w+)(?::(.*))?\]/);
      if (m) executeTool(m[1], m[2] || '');
    }
  }
}

// ---- Markdown 渲染 ----
function renderMarkdown(text) {
  if (!text) return '';
  var html = text;

  // 代码块 ```...```
  html = html.replace(/```(\w*)\n([\s\S]*?)```/g, function(m, lang, code) {
    return '<pre class="code-block"><code class="language-' + (lang||'') + '">' + escapeHtml(code.trimEnd()) + '</code></pre>';
  });

  // 行内代码 `...`
  html = html.replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>');

  // 粗体 **...**
  html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');

  // 斜体 *...*
  html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');

  // 无序列表 - item
  html = html.replace(/^- (.+)$/gm, '<li>$1</li>');
  html = html.replace(/(<li>.*<\/li>\n?)+/g, '<ul class="md-list">$&</ul>');

  // 表格 |...|...|
  html = html.replace(/^\|(.+)\|$/gm, function(m) {
    var cells = m.split('|').filter(function(c) { return c.trim(); });
    if (cells.every(function(c) { return /^[-:]+$/.test(c.trim()); })) return ''; // 分隔行
    return '<tr>' + cells.map(function(c) { return '<td>' + c.trim() + '</td>'; }).join('') + '</tr>';
  });
  html = html.replace(/(<tr>.*<\/tr>\n?)+/g, '<table class="md-table">$&</table>');

  // 普通换行 → <br>
  html = html.replace(/\n/g, '<br>');

  return html;
}

function escapeHtml(str) {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ---- Diff 渲染 ----
function renderDiff(original, modified) {
  var oLines = original.split('\n');
  var mLines = modified.split('\n');
  var html = '<div class="diff-view">';
  var maxLen = Math.max(oLines.length, mLines.length);
  for (var i = 0; i < Math.min(maxLen, 30); i++) {
    var oLine = i < oLines.length ? oLines[i] : '';
    var mLine = i < mLines.length ? mLines[i] : '';
    if (oLine === mLine) {
      html += '<div class="diff-line"><span class="diff-num">' + (i+1) + '</span><span class="diff-same"> ' + escapeHtml(oLine) + '</span></div>';
    } else {
      if (oLine) html += '<div class="diff-line diff-removed"><span class="diff-num">' + (i+1) + '</span><span>-</span> ' + escapeHtml(oLine) + '</div>';
      if (mLine) html += '<div class="diff-line diff-added"><span class="diff-num">' + (i+1) + '</span><span>+</span> ' + escapeHtml(mLine) + '</div>';
    }
  }
  if (maxLen > 30) html += '<div class="diff-more">… 省略 ' + (maxLen - 30) + ' 行</div>';
  html += '</div>';
  return html;
}

// ---- 消息 ----
function addMsg(role, text) {
  var div = document.createElement('div');
  div.className = 'message ' + role;
  if (role === 'assistant') {
    div.innerHTML = renderMarkdown(text);
  } else {
    div.innerHTML = renderMarkdown(text);
  }
  document.getElementById('messages').appendChild(div);
  document.getElementById('messages').scrollTop = document.getElementById('messages').scrollHeight;
}

// ---- 工具调用 ----
function parseToolCalls(text) {
  var lines = text.split('\n');
  for (var i = 0; i < lines.length; i++) {
    var line = lines[i].trim();
    if (line.startsWith('[TOOL:')) {
      var m = line.match(/\[TOOL:(\w+)(?::(.*))?\]/);
      if (m) executeTool(m[1], m[2] || '');
    }
  }
}

async function callApiWithRetry(url, body, retries) {
  retries = retries || 2;
  for (var i = 0; i < retries; i++) {
    try {
      var resp = await fetch(url, { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify(body||{}) });
      var data = await resp.json();
      if (data.ok !== false || i === retries - 1) return data;
      if (i < retries - 1) await new Promise(function(r) { setTimeout(r, 300); });
    } catch(e) {
      if (i === retries - 1) throw e;
      await new Promise(function(r) { setTimeout(r, 500); });
    }
  }
}

async function executeTool(tool, arg) {
  switch(tool) {
    case 'exec':
      addMsg('system', '🔧 执行: ' + arg);
      var r = await fetch('/api/exec', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({command:arg, cwd:'.'}) });
      var d = await r.json();
      addMsg('system', (d.ok ? '✓' : '✗') + ' ' + (d.stdout || d.stderr || '').substring(0, 500));
      break;

    case 'read':
      var parts = arg.split(':'); var path = parts[0].trim();
      var start = parseInt(parts[1]) || 0; var end = parseInt(parts[2]) || 0;
      var r = await fetch('/api/read', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({path:path, start:start, end:end}) });
      var d = await r.json();
      if (d.ok) addMsg('system', '📄 ' + path + ' (' + d.total_lines + '行)\n' + (d.lines||[]).join('\n').substring(0, 2000));
      else addMsg('system', '❌ ' + d.error);
      break;

    case 'search':
      if (!arg || arg === 'null') { addMsg('system', '🔍 请输入搜索关键词（在当前项目中 ripgrep 搜索）'); break; }
      var r = await fetch('/api/search', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({pattern:arg, path:'.'}) });
      var d = await r.json();
      if (d.ok && d.matches.length) addMsg('system', '🔍 ' + arg + ' (' + d.count + '条):\n' + d.matches.join('\n').substring(0, 2000));
      else addMsg('system', '🔍 \"' + arg + '\" 在当前项目中无匹配（ripgrep 搜索源文件）');
      break;

    case 'web':
      if (!arg || arg === 'null') { addMsg('system', '🌐 请输入搜索关键词（GitHub+Gitee+DuckDuckGo 三层搜索）'); break; }
      var r = await fetch('/api/web-search', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({query:arg}) });
      var d = await r.json();
      if (d.ok && d.results.length) addMsg('system', '🌐 搜索 \"' + arg + '\":\n' + d.results.join('\n').substring(0, 2000));
      else addMsg('system', '🌐 \"' + arg + '\" 无结果');
      break;

    case 'lsp':
      var r = await fetch('/api/lsp', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({file:arg||''}) });
      var d = await r.json();
      if (d.ok && d.errors.length) addMsg('system', '🔬 ' + d.count + '错误:\n' + d.errors.map(function(e){return e.file+':'+e.line+' - '+e.message;}).join('\n').substring(0, 2000));
      else addMsg('system', '🔬 无错误');
      break;

    case 'auto-fix':
      addMsg('system', '🔧 启动自动修复循环…');
      var es = new EventSource('/api/auto-fix');
      es.onmessage = function(evt) {
        try { var d = JSON.parse(evt.data); if (d.type==='chunk') addMsg('system', d.content); if (d.type==='done') { addMsg('system', d.message); es.close(); } if (d.type==='error') { addMsg('system', '❌ '+d.message); es.close(); } } catch(e){}
      };
      break;

    case 'rollback':
      var r = await fetch('/api/rollback', { method:'POST' });
      var d = await r.json();
      addMsg('system', '已回滚 ' + d.rolled_back + ' 个文件');
      break;

    case 'parallel':
      var paths = arg.split(',').map(function(p) { return p.trim(); });
      var r = await fetch('/api/parallel', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({paths:paths}) });
      var d = await r.json();
      if (d.ok) d.results.forEach(function(res) {
        if (res.ok) addMsg('system', '📄 ' + res.path + ' (' + res.total + '行)\n' + (res.lines||[]).join('\n').substring(0, 1000));
        else addMsg('system', '❌ ' + res.path + ': ' + res.error);
      });
      break;

    case 'infer':
      var r = await fetch('/api/infer', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({target: arg}) });
      var d = await r.json();
      if (d.ok && d.findings.length) addMsg('system', '🔬 推理 ' + arg + ':\n' + d.findings.join('\n'));
      else addMsg('system', '🔬 未找到 ' + arg);
      break;

    case 'edit':
      var parts = arg.split('::'); var path = parts[0].trim();
      var start = parseInt(parts[1]) || 0; var end = parseInt(parts[2]) || 0;
      var content = parts.slice(3).join('::').trim() || prompt('新内容（\\n换行）：') || '';
      var r = await fetch('/api/edit', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({path:path, start:start, end:end, content:content}) });
      var d = await r.json();
      addMsg('system', d.ok ? '✏ ' + d.replaced + ' (' + d.total_lines + '行)' : '❌ ' + d.error);
      break;

    case 'snap':
      var r = await fetch('/api/snapshot', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({action:'list'}) });
      var d = await r.json();
      if (d.ok && d.snapshots.length) addMsg('system', '📸 快照:\n' + d.snapshots.map(function(s) { return s.file + ' @' + s.at; }).join('\n'));
      else addMsg('system', '📸 无快照');
      break;

    case 'structure':
      var r = await fetch('/api/structure');
      var d = await r.json();
      if (d.ok) addMsg('system', '🏗 项目结构 (' + d.summary + '):\n' + d.modules.map(function(m) { return '  ' + m.module + '/: ' + (m.files||[]).join(', '); }).join('\n'));
      break;

    case 'explore':
      var r = await fetch('/api/explore');
      var d = await r.json();
      if (d.ok && d.findings.length) addMsg('system', '🔍 项目探查:\n' + d.findings.join('\n'));
      else addMsg('system', '🔍 探查无结果');
      break;

    case 'save':
      var r = await fetch('/api/save-context', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({content:arg}) });
      var d = await r.json();
      addMsg('system', d.ok ? '已保存' : '保存失败');
      break;
  }
}

// ---- 更新 ----
async function checkUpdate() {
  try {
    var r = await fetch('/api/update-check');
    var d = await r.json();
    if (d.update_available) {
      document.getElementById('update-banner').style.display = 'flex';
      document.getElementById('update-text').textContent = '🔔 v' + d.latest + ' 可用！当前 v' + d.current;
      document.getElementById('update-btn').onclick = function() { doUpdate(); };
    }
  } catch(e) {}
}

async function doUpdate() {
  document.getElementById('update-text').textContent = '⬇ 下载中…';
  var r = await fetch('/api/update-now', { method:'POST' });
  var d = await r.json();
  if (d.ok) addMsg('system', '✅ ' + d.message);
  else addMsg('system', '❌ ' + d.error);
}

// ---- 右侧面板 ----
async function refreshRight() {
  try {
    var s = await fetch('/api/status'); var sd = await s.json();
    document.getElementById('topbar-cost').textContent = '💰 ¥' + sd.cost.toFixed(4);
    document.getElementById('topbar-hit').textContent = (sd.hit_rate*100).toFixed(0) + '%';
    document.getElementById('cost-val').textContent = '¥' + sd.cost.toFixed(4);
    document.getElementById('hit-val').textContent = '命中 ' + (sd.hit_rate*100).toFixed(0) + '%';

    // 会话统计
    document.getElementById('stat-turns').textContent = sd.turns || 0;
    document.getElementById('stat-tokens').textContent = formatTokens(sd.total_tokens || 0);
    document.getElementById('stat-cache').textContent = (sd.hit_rate*100).toFixed(0) + '%';
    document.getElementById('stat-model').textContent = sd.model || '-';

    if (sd.evolution) {
      document.getElementById('evo-mini').innerHTML = '经验: ' + sd.evolution.experiences + ' | SOP: ' + sd.evolution.sops + ' | 成功率: ' + (sd.evolution.success_rate*100).toFixed(0) + '%';
    }

    var c = await fetch('/api/cost'); var cd = await c.json();
    document.getElementById('savings-val').textContent = '节省 ' + cd.vs_claude_savings_pct.toFixed(0) + '%';

    // 项目摘要
    var f = await fetch('/api/project'); var fd = await f.json();
    if (fd.ok && fd.recent_commits && fd.recent_commits.length) {
      var el = document.getElementById('recent-commits');
      el.innerHTML = fd.recent_commits.slice(0, 5).map(function(c) {
        return '<div class="commit-row"><span class="commit-hash">' + c.hash + '</span><span class="commit-msg">' + c.message.substring(0, 40) + '</span><span class="commit-date">' + c.date + '</span></div>';
      }).join('');
    }

    loadSessions();
  } catch(e) {}
}

function formatTokens(n) {
  if (n >= 1000000) return (n/1000000).toFixed(1) + 'M';
  if (n >= 1000) return (n/1000).toFixed(1) + 'K';
  return n.toString();
}

// ---- 复盘 ----
function setupReview() {
  document.getElementById('session-end-btn').addEventListener('click', showReview);
  document.getElementById('review-submit-btn').addEventListener('click', submitReview);
  document.getElementById('review-skip-btn').addEventListener('click', function() { document.getElementById('review-modal').style.display = 'none'; });
}

function showReview() {
  var m = document.getElementById('review-modal');
  var p = document.getElementById('review-preview');
  var msgs = document.querySelectorAll('#messages .message');
  var turns = 0, samples = [];
  msgs.forEach(function(msg) {
    if (msg.classList.contains('user') || msg.classList.contains('assistant')) turns++;
    if (samples.length < 3) samples.push((msg.textContent||'').substring(0, 60));
  });
  p.innerHTML = '<div>对话轮次: ' + Math.floor(turns/2) + '</div><div>摘要: ' + samples.join(' | ') + '...</div><div style="color:var(--green);margin-top:8px">不上传任何代码、路径、Key</div>';
  m.style.display = 'flex';
}

async function submitReview() {
  var msgs = document.querySelectorAll('#messages .message');
  var turns = [];
  msgs.forEach(function(msg) {
    if (msg.classList.contains('user') || msg.classList.contains('system')) turns.push(msg.textContent.substring(0, 100));
  });
  var r = await fetch('/api/review/submit', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({turns:turns, project:document.title||''}) });
  var d = await r.json();
  var el = document.getElementById('review-msg');
  el.textContent = d.ok ? '✅ 已提交（经验' + d.experiences + '条/SOP'+d.sops+'条）' : '提交失败';
  el.style.color = d.ok ? 'var(--green)' : 'var(--red)';
  if (d.ok) { saveSession(); setTimeout(function() { document.getElementById('review-modal').style.display = 'none'; }, 1500); }
}

async function saveSession() {
  var msgs = document.querySelectorAll('#messages .message');
  var all = [];
  msgs.forEach(function(m) { all.push((m.textContent||'').substring(0, 100)); });
  await fetch('/api/session/save', { method:'POST', headers:{'Content-Type':'application/json'}, body:JSON.stringify({title: document.title||'对话', messages:all}) });
}

async function loadSessions() {
  try {
    var r = await fetch('/api/sessions');
    var d = await r.json();
    if (d.ok && d.sessions.length) {
      var html = '';
      d.sessions.forEach(function(s) {
        html += '<div style="margin:4px 0;cursor:pointer;color:var(--text-dim)" onclick="addMsg(\'system\',\'会话: '+s.title+' ('+s.turns+'轮 '+s.date+')\')">📋 '+s.title+' ('+s.turns+'轮)<br><span style="font-size:10px">'+s.date+'</span></div>';
      });
      document.getElementById('recent-sessions').innerHTML = html;
    }
  } catch(e) {}
}
