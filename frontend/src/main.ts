import { mount } from 'svelte'
import App from './App.svelte'
import { createServices } from './app/bootstrap'
import './styles.css'

const target = document.getElementById('app')

if (!target) {
  throw new Error('Application root is missing')
}

// i18n and the adapter are ready before the first render, so no screen flashes untranslated keys.
void createServices().then(({ services, start }) => {
  mount(App, { target, props: { services, start } })
})
