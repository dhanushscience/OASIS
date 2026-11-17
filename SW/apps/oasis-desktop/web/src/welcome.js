import { mount } from 'svelte';
import './tokens.css';
import Welcome from './components/Welcome.svelte';

mount(Welcome, { target: /** @type {HTMLElement} */ (document.getElementById('app')) });
