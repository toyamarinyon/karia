import { useState } from 'react'
import styles from './App.module.css'

export default function App() {
  const [count, setCount] = useState(0)
  return (
    <main className={styles.page}>
      <p className={styles.label}>CSS MODULES × CSS LANGUAGE SERVICE</p>
      <h1>普通のCSSで、試そう。</h1>
      <p>背景色は tokens.css、ボタンの色はこのCSS Moduleで定義しています。</p>
      <button className={styles.button} onClick={() => setCount(count + 1)}>
        クリック回数: {count}
      </button>
      <p className={styles.note}>補完の実測: <code>npm run probe:css</code></p>
    </main>
  )
}
