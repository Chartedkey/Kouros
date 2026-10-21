import React from 'react';
import ReactDOM from 'react-dom/client';
// Fonts are bundled with the app — no requests to Google Fonts.
import '@fontsource/unbounded/500.css';
import '@fontsource/unbounded/700.css';
import '@fontsource/manrope/400.css';
import '@fontsource/manrope/500.css';
import '@fontsource/manrope/600.css';
import '@fontsource/manrope/700.css';
import './styles.css';
import App from './App';
import { initZoom } from './lib/zoom';
import { initMasonry } from './lib/masonry';
import { initBoard } from './lib/board';
import { initHelpTips } from './lib/helptips';

initZoom();
initBoard();
initMasonry();
initHelpTips();

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
