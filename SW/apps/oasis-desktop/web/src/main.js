import { mount } from 'svelte';
import './tokens.css';
import './legacy/legacy.css';
import './keybuttons.js';
import App from './App.svelte';

mount(App, { target: /** @type {HTMLElement} */ (document.getElementById('app')) });
