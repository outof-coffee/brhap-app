import { mount } from 'svelte'
// Pico first, so our own rules in app.css and the components win over it.
import '@picocss/pico/css/pico.min.css'
import './app.css'
import App from './App.svelte'

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
