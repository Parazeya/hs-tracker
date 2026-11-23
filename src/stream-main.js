// This entry has no Tauri, app controls, audio or native window dependencies.
import { mount } from 'svelte';
import StreamView from './StreamView.svelte';
mount(StreamView, { target: document.getElementById('app') });
