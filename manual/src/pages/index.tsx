import Head from "@docusaurus/Head";
import Link from "@docusaurus/Link";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import type { CSSProperties, ReactNode } from "react";

import Screenshot from "../components/Screenshot";

import "../css/home.css";

const TITLE = "KizunaShelf — a shelf for everything you love";
const DESCRIPTION =
  "KizunaShelf is a personal library for everything you watch, play, read, and listen to, kept as plain Markdown files that stay yours.";

const METADATA_SOURCES = [
  "TMDB",
  "TheTVDB",
  "AniList",
  "MyAnimeList",
  "Bangumi",
  "IGDB",
  "Steam",
  "BoardGameGeek",
  "MusicBrainz",
  "Apple Music",
  "Apple Podcasts",
  "Discogs",
  "NeoDB",
  "Open Library",
  "Google Books",
  "Hardcover",
  "MangaUpdates",
  "Comic Vine",
];

const IMPORT_SOURCES = [
  "Bangumi",
  "MyAnimeList",
  "AniList",
  "Kitsu",
  "Trakt",
  "Steam",
  "IMDb CSV",
  "Goodreads CSV",
  "Yamtrack CSV",
];

/**
 * The landing page. Deliberately rendered *without* the Docusaurus <Layout>: it
 * is its own standalone design (see `src/css/home.css`), not a docs page with a
 * navbar. The `home-page` body class is what scopes that stylesheet.
 */
export default function Home(): ReactNode {
  const { siteConfig } = useDocusaurusContext();

  return (
    <>
      <Head>
        <body className="home-page" />
        <title>{TITLE}</title>
        <meta name="description" content={DESCRIPTION} />
        <meta name="theme-color" media="(prefers-color-scheme: light)" content="#FFFFFF" />
        <meta name="theme-color" media="(prefers-color-scheme: dark)" content="#0A0A0A" />
        <meta property="og:type" content="website" />
        <meta property="og:title" content={TITLE} />
        <meta property="og:description" content={DESCRIPTION} />
        <meta property="og:image" content={`${siteConfig.url}/images/og-image.png`} />
        <meta name="twitter:card" content="summary" />
        <link
          rel="preload"
          href="/assets/fonts/bookman-demi.woff2"
          as="font"
          type="font/woff2"
          crossOrigin=""
        />
        <link
          rel="preload"
          href="/assets/fonts/bookman-lightitalic.woff2"
          as="font"
          type="font/woff2"
          crossOrigin=""
        />
        <link
          rel="preload"
          href="/assets/fonts/c059-roman.woff2"
          as="font"
          type="font/woff2"
          crossOrigin=""
        />
        <link
          rel="preload"
          href="/assets/fonts/jbmono.woff2"
          as="font"
          type="font/woff2"
          crossOrigin=""
        />
      </Head>

      <a className="skip-link" href="#main">
        Skip to content
      </a>

      <header className="site">
        <div className="wrap">
          <span className="wordmark">
            <img src="/assets/icon.webp" alt="" className="only-light" />
            <img src="/assets/icon-dark.webp" alt="" className="only-dark" />
            KizunaShelf
          </span>
          <nav className="site" aria-label="Sections">
            <a href="#shelf">the shelf</a>
            <a href="#files">the files</a>
            <a href="#time">the record</a>
            <a href="#apps">the apps</a>
            <Link to="/introduction/">the manual</Link>
          </nav>
        </div>
      </header>

      <main id="main">
        <section className="hero">
          <div className="wrap">
            <div>
              <h1>
                A shelf for everything you&nbsp;love.
                <span className="stays">— and it stays yours.</span>
              </h1>
              <p className="lede">
                Keep what you watch, play, read, and listen to in one personal library. Connect
                stories, games, music, and the people behind them. Your collection stays in plain
                Markdown files, ready to open in Obsidian or any text editor.
              </p>
              <div className="hero-ctas">
                <a className="btn primary" href="https://testflight.apple.com/join/hE7k3sWd">
                  Join the iOS beta
                </a>
                <Link className="btn ghost" to="/introduction/">
                  Read the manual
                </Link>
              </div>
              <p className="fine" style={{ marginTop: "1.1rem" }}>
                desktop &amp; web coming soon · Android planned
              </p>
            </div>
            <figure className="hero-fig">
              <div className="phone">
                <img
                  src="/assets/screenshot.webp"
                  alt="KizunaShelf on iPhone, with activity tiles and Watching Anime and Playing Games shelves"
                  width={720}
                  height={1561}
                />
              </div>
            </figure>
          </div>
        </section>

        <section id="shelf">
          <div className="wrap">
            <div className="section-head">
              <h2>It starts familiar. It ends up yours.</h2>
            </div>
            <p>
              Start with anime, games, books, movies, and albums. Choose the details you want to
              keep: a status, a rating, a cover, a date, or a note for later.
            </p>
            <p>
              If your world needs goods, cards, voice actors, live events, or trains, shoes,
              museums, coffee beans, or something nobody else would think to model, KizunaShelf
              gives you the grammar to describe it.
            </p>

            <div className="bookshelf" aria-hidden="true">
              <div className="books">
                <span className="spine s-red tall">Anime</span>
                <span className="spine s-amber mid">Games</span>
                <span className="spine s-teal">Books</span>
                <span className="spine s-blue mid">Movies</span>
                <span className="spine s-purple short">Music</span>
                <span className="spine s-red short">Manga</span>
                <span className="spine lean short">yours?</span>
              </div>
              <div className="plank" />
              <div className="shelf-feet">
                <i />
                <i />
              </div>
            </div>
          </div>
        </section>

        <section id="files">
          <div className="wrap split">
            <div>
              <h2>Your library is a folder of text files.</h2>
              <p>
                Each entity has a Markdown file for its details and your notes. Open it in Obsidian,
                sync the folder between devices, and back it up with your other files.
              </p>
              <p>
                KizunaShelf adds covers, search, filters, and a calendar to those same notes. Your
                files remain readable without the app.
              </p>
            </div>
            <figure className="filecard tilt" style={{ "--dot": "var(--teal)" } as CSSProperties}>
              <div className="bar">Anime/BanG Dream! YUME∞MITA.md</div>
              <pre tabIndex={0}>
                <span className="dim">---</span>
                {"\n"}
                <span className="k">title_original</span>
                <span className="dim">:</span> <span className="v">バンドリ！ゆめ∞みた</span>
                {"\n"}
                <span className="k">status</span>
                <span className="dim">:</span> <span className="v">Watching</span>
                {"\n"}
                <span className="k">season</span>
                <span className="dim">:</span> <span className="v">Summer 2026</span>
                {"\n"}
                <span className="k">franchise</span>
                <span className="dim">:</span> <span className="wl">{'["[[BanG Dream!]]"]'}</span>
                {"\n"}
                <span className="k">anilist</span>
                <span className="dim">:</span>{" "}
                <span className="wl">https://anilist.co/anime/198376</span>
                {"\n"}
                <span className="dim">---</span>
                {"\n\n"}
                <span className="prose-line">夢はパワー!夢はパワー!</span>
              </pre>
            </figure>
          </div>
        </section>

        <section id="connections">
          <div className="wrap">
            <div className="section-head">
              <h2>Things belong to each other.</h2>
            </div>
            <p>
              Keep Steins;Gate games and anime together under a franchise. Link Kurisu Makise to her
              voice actor, Asami Imai. Follow those connections through your library — they are
              ordinary <span className="inline-link-example">[[wikilinks]]</span> in your notes.
            </p>

            <figure className="graph-fig" tabIndex={0}>
              <svg
                viewBox="0 0 660 320"
                role="img"
                aria-label="Steins;Gate connects to Steins;Gate Re：Boot, Steins;Gate 0 (Anime), Hacking to the Gate, and Kurisu Makise. Kurisu links to voice actor Asami Imai."
              >
                <path className="edge" d="M 330 150 C 280 110, 220 80, 160 72" />
                <path className="edge" d="M 330 150 C 360 100, 420 68, 480 60" />
                <path className="edge" d="M 330 150 C 260 190, 200 220, 148 236" />
                <path className="edge" d="M 330 150 C 400 180, 440 210, 484 238" />
                <path className="edge" d="M 484 238 C 540 250, 570 258, 600 262" />
                <circle cx="330" cy="150" r="30" fill="var(--red)" />
                <circle cx="160" cy="72" r="22" fill="var(--teal)" />
                <circle cx="480" cy="60" r="22" fill="var(--blue)" />
                <circle cx="148" cy="236" r="22" fill="var(--amber)" />
                <circle cx="484" cy="238" r="22" fill="var(--purple)" />
                <circle cx="600" cy="262" r="14" fill="var(--wood)" />
                <text className="node-label" x="330" y="202" textAnchor="middle">
                  Steins;Gate
                </text>
                <text className="node-sub" x="330" y="219" textAnchor="middle">
                  franchise
                </text>
                <text className="node-label" x="160" y="31" textAnchor="middle">
                  Steins;Gate Re：Boot
                </text>
                <text className="node-sub" x="160" y="47" textAnchor="middle">
                  game
                </text>
                <text className="node-label" x="480" y="19" textAnchor="middle">
                  Steins;Gate 0 (Anime)
                </text>
                <text className="node-sub" x="480" y="35" textAnchor="middle">
                  anime
                </text>
                <text className="node-label" x="148" y="285" textAnchor="middle">
                  Hacking to the Gate
                </text>
                <text className="node-sub" x="148" y="302" textAnchor="middle">
                  music
                </text>
                <text className="node-label" x="448" y="288" textAnchor="middle">
                  Kurisu Makise
                </text>
                <text className="node-sub" x="448" y="305" textAnchor="middle">
                  character
                </text>
                <text className="node-label" x="602" y="292" textAnchor="middle">
                  Asami Imai
                </text>
                <text className="node-sub" x="602" y="308" textAnchor="middle">
                  artist · voice-by
                </text>
              </svg>
            </figure>
          </div>
        </section>

        <section id="time">
          <div className="wrap split">
            <figure
              className="filecard tilt-r"
              style={{ "--dot": "var(--purple)" } as CSSProperties}
            >
              <div className="bar">Daily Notes/2026-01-03.md</div>
              <pre tabIndex={0}>
                <span className="prose-line">Speedrun trip in Tokyo. What a big day!</span>
                {"\n"}
                <span className="dim">-</span> <span className="v">08:30</span> Arrived at Tokyo Big
                Sight
                {"\n"}
                <span className="dim">-</span> <span className="v">13:00</span> Odaiba Statue of
                Liberty 🗽{"\n"}
                <span className="dim">-</span> <span className="v">14:30</span>{" "}
                <span className="wl">[[Nijigasaki The Movie Part 2]]</span> 🌈{"\n  "}
                <span className="dim">
                  {"— The stage at Umeda Sky Building\n    is wonderful!"}
                </span>
                {"\n"}
                <span className="dim">-</span> <span className="v">17:00</span>{" "}
                <span className="wl">[[Poppin&apos;Party New Year LIVE]]</span> 🎸{"\n  "}
                <span className="dim">
                  {"— "}
                  <span className="wl">[[FIRE BIRD]]</span>
                  {" is a big surprise! 🔥"}
                </span>
                {"\n"}
              </pre>
            </figure>
            <div>
              <h2>
                It remembers <em>when</em>.
              </h2>
              <p>
                See releases and plans on a calendar. Check off an episode or log a session, then
                find it again in your history. A link in your daily note is enough to connect an
                ordinary day with something you loved.
              </p>
            </div>
          </div>
        </section>

        <section className="kizuna">
          <div className="wrap">
            <div className="kanji" aria-hidden="true">
              絆
            </div>
            <div>
              <span className="reading">kizuna — きずな</span>
              <p>
                <em>Kizuna</em> means bond. Between a story and the people who made it. Between a
                rainy Friday and the episode you watched that night. Between you and the things you
                keep.
              </p>
              <p className="fine" style={{ marginTop: "1.4rem" }}>
                <a href="https://mudkip.me/2026/07/16/Introduction-to-KizunaShelf/">
                  Read the story behind KizunaShelf →
                </a>
              </p>
            </div>
          </div>
        </section>

        <section id="apps">
          <div className="wrap">
            <div className="section-head">
              <h2>At your desk or in your pocket.</h2>
            </div>
            <ul className="app-list">
              <li>
                <div className="app-name">
                  iPhone &amp; iPad <span className="status">on TestFlight</span>
                </div>
                <div>
                  <p>
                    Browse and edit on iPhone or iPad. Use widgets, reminders, Spotlight, Safari
                    sharing, and Shortcuts to keep your library close.
                  </p>
                  <p>
                    <a href="https://testflight.apple.com/join/hE7k3sWd">
                      Join the beta on TestFlight →
                    </a>
                  </p>
                </div>
              </li>
              <li>
                <div className="app-name">
                  Desktop <span className="status soon">coming soon</span>
                </div>
                <div>
                  <p>
                    Open or create vaults with an open source desktop app on your Mac, Linux or
                    Windows PC.
                  </p>
                </div>
              </li>
              <li>
                <div className="app-name">
                  Self-hosted web <span className="status soon">coming soon</span>
                </div>
                <div>
                  <p>
                    Served from your container to any browser, for people whose shelf lives on their
                    NAS.
                  </p>
                </div>
              </li>
              <li>
                <div className="app-name">
                  Android <span className="status planned">planned</span>
                </div>
                <div>
                  <p>An Android app is planned.</p>
                </div>
              </li>
            </ul>
          </div>
        </section>

        <section id="sources">
          <div className="wrap">
            <div className="section-head">
              <h2>Fill in the details. Bring your history.</h2>
            </div>
            <div className="source-groups">
              <div>
                <h3>Find titles, covers, and more</h3>
                <p>
                  Use metadata sources to add an entity or refresh its details. Search by title, or
                  paste a supported link. Some sources need an API key or a direct URL.
                </p>
                <ul className="stickers" aria-label="Metadata sources">
                  {METADATA_SOURCES.map((source) => (
                    <li key={source}>{source}</li>
                  ))}
                </ul>
                <Link to="/reference/providers/">Metadata sources and requirements →</Link>
              </div>
              <div id="imports">
                <h3>Import a collection you already keep</h3>
                <p>
                  Bring in a public profile or an exported library, with supported ratings,
                  progress, and notes. Review the additions before anything is saved.
                </p>
                <ul className="stickers" aria-label="Import sources">
                  {IMPORT_SOURCES.map((source) => (
                    <li key={source}>{source}</li>
                  ))}
                </ul>
                <Link to="/features/import/">How importing works →</Link>
              </div>
            </div>
          </div>
        </section>

        <section id="closer-look">
          <div className="wrap">
            <div className="section-head">
              <h2>A closer look on iPhone.</h2>
            </div>
            <div className="ios-gallery">
              {/* Add ios image paths and matching iosAlt descriptions when captures are ready. */}
              <Screenshot platforms="ios" caption="Your library, ready to browse." />
              <Screenshot platforms="ios" caption="Episode tracking and your notes." />
              <Screenshot platforms="ios" caption="Your activity history." />
            </div>
          </div>
        </section>
      </main>

      <footer className="site">
        <div className="wrap">
          <img src="/assets/icon.webp" alt="" className="only-light" />
          <img src="/assets/icon-dark.webp" alt="" className="only-dark" />
          <p className="foot-small">KizunaShelf · Forever for dreaming</p>
        </div>
      </footer>
    </>
  );
}
