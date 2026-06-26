export function formatHmsFromDate(date) {
  return date.toISOString().slice(11, 19);
}

export function currentHmsPrefix() {
  return `[${formatHmsFromDate(new Date(Date.now()))}] `;
}
