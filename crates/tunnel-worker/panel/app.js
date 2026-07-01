// Every request carries X-Tunnel-CSRF; mutations are rejected without it by the
// worker as a cross-origin CSRF defense. Sending it on GETs too is harmless.
const api = (path, opts = {}) =>
  fetch(path, {
    credentials: "same-origin",
    headers: { "Content-Type": "application/json", "X-Tunnel-CSRF": "1" },
    ...opts,
  });

async function login() {
  const secret = document.getElementById("secret").value;
  const r = await api("/admin/login", { method: "POST", body: JSON.stringify({ secret }) });
  if (r.ok) {
    document.getElementById("login").hidden = true;
    document.getElementById("dashboard").hidden = false;
    refresh();
  } else alert("login failed");
}

async function refresh() {
  const clients = await (await api("/admin/clients")).json();
  document.getElementById("clients").innerHTML = clients
    .map(
      (c) => `<li><code>${c.id}</code> ${c.name} <code>${c.token_prefix}…</code> ${c.disabled ? "(disabled)" : ""}
      <button data-status="${c.id}">status</button>
      <button data-del="${c.id}">delete</button></li>`
    )
    .join("");
  const routes = await (await api("/admin/routes")).json();
  document.getElementById("routes").innerHTML = routes
    .map(
      (r) => `<li>${r.kind}:${r.matcher} → ${r.target} (${r.client_id})
      <button data-delroute="${r.id}">x</button></li>`
    )
    .join("");
}

async function showStatus(id) {
  const s = await (await api(`/admin/clients/${id}/status`)).json();
  document.getElementById("status").hidden = false;
  document.getElementById("status-summary").textContent =
    `connections: ${s.connections}, last_seen: ${s.last_seen}`;
  document.querySelector("#status-recent tbody").innerHTML = (s.recent || [])
    .map(
      (r) => `<tr><td>${r.ts}</td><td>${r.method}</td><td>${r.path}</td><td>${r.status}</td><td>${r.latency_ms}</td><td>${r.target}</td></tr>`
    )
    .join("");
}

document.getElementById("login-btn").onclick = login;
document.getElementById("status-close").onclick = () => {
  document.getElementById("status").hidden = true;
};
document.getElementById("new-client").onsubmit = async (e) => {
  e.preventDefault();
  const name = document.getElementById("client-name").value;
  const r = await (await api("/admin/clients", { method: "POST", body: JSON.stringify({ name }) })).json();
  prompt("Copy this token now; it will not be shown again:", r.token);
  refresh();
};
document.getElementById("new-route").onsubmit = async (e) => {
  e.preventDefault();
  const body = JSON.stringify({
    client_id: document.getElementById("route-client").value,
    kind: document.getElementById("route-kind").value,
    matcher: document.getElementById("route-matcher").value,
    target: document.getElementById("route-target").value,
  });
  await api("/admin/routes", { method: "POST", body });
  refresh();
};
document.body.addEventListener("click", async (e) => {
  if (e.target.dataset.status) return showStatus(e.target.dataset.status);
  if (e.target.dataset.del) {
    await api(`/admin/clients/${e.target.dataset.del}`, { method: "DELETE" });
    refresh();
  }
  if (e.target.dataset.delroute) {
    await api(`/admin/routes/${e.target.dataset.delroute}`, { method: "DELETE" });
    refresh();
  }
});
