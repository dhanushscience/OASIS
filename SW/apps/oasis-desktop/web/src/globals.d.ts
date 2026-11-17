declare module '*.css';
declare module '*.html?raw' { const html: string; export default html; }
interface Window { __TAURI__: any }
