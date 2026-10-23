import React from 'react';
import clsx from 'clsx';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import styles from './index.module.css';

const TAGLINE = 'A general-purpose agent that a web page cannot redirect.';

const FEATURES = [
  {
    title: 'Structural resistance to indirect prompt injection',
    text: 'A web page cannot redirect an agent working on your behalf. Content nobody vouched for never reaches the part that decides what to do next.',
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

function Hero({repository}) {
  const art = useBaseUrl('/img/bravebot-hero.svg');
  const stars = useBaseUrl('/img/bravebot-stars.svg');
  return (
    <header className={styles.hero} style={{backgroundImage: `url(${stars})`}}>
      <h1 className={styles.title}>
        <img className={styles.art} src={art} width="1280" height="314" alt="bravebot" />
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
      <p className={styles.install}>
        <code>npm install -g @brave/bravebot</code>
      </p>
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
