import React, {useState} from 'react';
import clsx from 'clsx';
import Layout from '@theme/Layout';
import TabItem from '@theme/TabItem';
import Tabs from '@theme/Tabs';
import ThemedImage from '@theme/ThemedImage';
import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import styles from './index.module.css';

const TAGLINE = 'A general-purpose agent that untrusted data cannot redirect.';

const FEATURES = [
  {
    title: 'Structural resistance to indirect prompt injection',
    text: "No untrusted data can redirect an agent working on your behalf, whether a web page, a file, a command's output or a tool's result. Content nobody vouched for never reaches the part that decides what to do next.",
    to: '/how-it-works',
    link: 'How it works',
  },
  {
    title: 'Nothing hidden',
    text: 'What it read and what it decided are on the screen, and every gate decision is recorded.',
    to: '/security/audit-trail',
    link: 'The audit trail',
  },
  {
    title: 'No lock-in',
    text: 'Brave Leo, Anthropic, OpenAI, Google Vertex AI, AWS Bedrock, OpenRouter and other OpenAI-compatible gateways, and local models in Ollama, LM Studio and llama-server.',
    to: '/category/reaching-a-model',
    link: 'Reaching a model',
  },
  {
    title: 'Easy to switch to',
    text: 'It reads AGENTS.md, or CLAUDE.md where there is none, and skills in .claude/skills. It can import Claude Code sessions.',
    to: '/customize/instructions',
    link: 'Instructions',
  },
  {
    title: 'Where you work',
    text: 'The terminal, an editor that speaks the Agent Client Protocol such as Zed, and scripts and pipes.',
    to: '/using/editors',
    link: 'Editors',
  },
  {
    title: 'The power of a shell without the risk',
    text: 'Command lines are compiled into the trust model rather than handed to a shell.',
    to: '/reference/run-tool',
    link: 'The run tool',
  },
];

// The commands the root README gives. The quickstart has the rest: a directory, a version, and
// checking a download by hand.
const INSTALL = [
  {value: 'npm', label: 'npm', command: 'npm install -g @brave/bravebot'},
  {
    value: 'unix',
    label: 'macOS and Linux',
    command: 'curl -fsSL https://raw.githubusercontent.com/brave/bravebot/main/install.sh | sh',
  },
  {
    value: 'windows',
    label: 'Windows (PowerShell)',
    command: 'irm https://raw.githubusercontent.com/brave/bravebot/main/install.ps1 | iex',
  },
];

function Command({command}) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // A browser that refuses the clipboard leaves the command to be selected by hand.
    }
  };
  return (
    <div className={styles.command}>
      <pre>
        <code>{command}</code>
      </pre>
      <button type="button" className={styles.copy} onClick={copy}>
        {copied ? 'Copied' : 'Copy'}
      </button>
    </div>
  );
}

function Hero({repository}) {
  const art = {
    dark: useBaseUrl('/img/bravebot-hero.svg'),
    light: useBaseUrl('/img/bravebot-hero-light.svg'),
  };
  // The stylesheet picks one of the two tiles for the colour mode.
  const stars = {
    '--stars-dark': `url(${useBaseUrl('/img/bravebot-stars.svg')})`,
    '--stars-light': `url(${useBaseUrl('/img/bravebot-stars-light.svg')})`,
  };
  return (
    <header className={styles.hero} style={stars}>
      <h1 className={styles.title}>
        <ThemedImage className={styles.art} sources={art} width="1280" height="314" alt="bravebot" />
      </h1>
      <p className={styles.tagline}>{TAGLINE}</p>
      <div className={styles.buttons}>
        <Link className={clsx(styles.button, styles.primary)} to="/quickstart">
          Get started
        </Link>
        <Link className={clsx(styles.button, styles.secondary)} to="/how-it-works">
          How it works
        </Link>
        <Link className={clsx(styles.button, styles.secondary)} href={repository}>
          GitHub
        </Link>
      </div>
      <div className={styles.install}>
        <Tabs defaultValue="npm">
          {INSTALL.map(({value, label, command}) => (
            <TabItem key={value} value={value} label={label}>
              <Command command={command} />
            </TabItem>
          ))}
        </Tabs>
        <p className={styles.more}>
          <Link to="/quickstart#install">All install options</Link>
        </p>
      </div>
    </header>
  );
}

export default function Home() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout description={TAGLINE}>
      <Hero repository={siteConfig.customFields.repository} />
      <main>
        <div className="container">
          <div className="alert alert--warning margin-top--lg" role="note">
            <strong>Not a released or supported Brave product.</strong> Brave does not release or
            support Brave Bot at this time. It is early, and it is being developed in the open. The
            binaries, the install script and the npm package are published for early testing and
            feedback only. Brave expects to release and support it as a product in the future.
          </div>
        </div>
        <section className={styles.features}>
          <div className="container">
            <h2>What makes it different</h2>
            <div className="row">
              {FEATURES.map(({title, text, to, link}) => (
                <div key={title} className={clsx('col col--4', styles.cell)}>
                  <div className={styles.card}>
                    <h3>{title}</h3>
                    <p>{text}</p>
                    <Link to={to}>{link}</Link>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}
