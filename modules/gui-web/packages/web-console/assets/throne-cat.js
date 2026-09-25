const throneCat = document.querySelector('[data-role="throne-cat"]');
const throneCatFrame = throneCat?.querySelector('[data-role="throne-cat-frame"]');

const throneCatFrames = Object.freeze({
  sleep: "./assets/ui-redesign/throne-cat/cat-01-sleep.png",
  stretch: "./assets/ui-redesign/throne-cat/cat-02-stretch.png",
  groom: "./assets/ui-redesign/throne-cat/cat-03-groom.png",
  tailLeft: "./assets/ui-redesign/throne-cat/cat-04-tail-left.png",
  tailRight: "./assets/ui-redesign/throne-cat/cat-05-tail-right.png",
  lookBack: "./assets/ui-redesign/throne-cat/cat-06-look-back.png",
  rest: "./assets/ui-redesign/throne-cat/cat-07-rest.png",
});

const idleSequence = Object.freeze([
  ["sleep", 16000],
  ["stretch", 1500],
  ["groom", 1200],
  ["groom", 1200],
  ["tailLeft", 500],
  ["tailRight", 500],
  ["tailLeft", 500],
  ["tailRight", 500],
  ["rest", 5000],
]);

let idleIndex = 0;
let idleTimer = 0;
let overrideState = null;

function showThroneCatFrame(name) {
  const source = throneCatFrames[name] || throneCatFrames.sleep;
  if (throneCatFrame && throneCatFrame.getAttribute("src") !== source) {
    throneCatFrame.setAttribute("src", source);
  }
  if (throneCat) throneCat.dataset.pose = name;
}

function scheduleThroneCatIdle() {
  window.clearTimeout(idleTimer);
  if (!throneCat || !throneCatFrame || overrideState) return;
  const [name, duration] = idleSequence[idleIndex % idleSequence.length];
  showThroneCatFrame(name);
  idleIndex += 1;
  idleTimer = window.setTimeout(scheduleThroneCatIdle, duration);
}

function setThroneCatOverride(state) {
  overrideState = state || null;
  throneCat?.classList.toggle("is-coronation", state === "coronation");
  if (state === "coronation") showThroneCatFrame("lookBack");
  else if (state === "hover") showThroneCatFrame("lookBack");
  else scheduleThroneCatIdle();
}

throneCat?.addEventListener("mouseenter", () => setThroneCatOverride("hover"));
throneCat?.addEventListener("mouseleave", () => setThroneCatOverride(null));
window.addEventListener("throne-cat-state", (event) => setThroneCatOverride(event.detail?.state));

scheduleThroneCatIdle();
