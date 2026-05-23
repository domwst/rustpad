export const USER_COLOR_HUES = [0, 24, 42, 92, 145, 185, 220, 260, 290, 330];

export function randomUserColorHue() {
  return USER_COLOR_HUES[Math.floor(Math.random() * USER_COLOR_HUES.length)];
}
