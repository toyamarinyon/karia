import { useState } from 'react'
import styles from './App.module.css'

export default function App() {
  const [count, setCount] = useState(0)
  return (
    <main className={styles.page}>
      <p className={styles.label}>CSS MODULES × CSS LANGUAGE SERVICE</p>
      <h1>Try it with plain CSS.</h1>
      <p>The background color comes from tokens.css; the button color is defined in this CSS Module.</p>
      <button className={styles.button} onClick={() => setCount(count + 1)}>
        Click count: {count}
      </button>
      <p className={styles.note}>Measured completion behavior: <code>npm run probe:css</code></p>
    </main>
  )
}
