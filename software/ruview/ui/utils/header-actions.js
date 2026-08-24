// Shared mount point for header controls.
// Falls back to the legacy status row so embedded/test shells keep working.
export function getHeaderActions() {
  return document.querySelector('[data-header-actions]') || document.querySelector('.header-info');
}
