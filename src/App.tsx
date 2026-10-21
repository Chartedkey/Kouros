import { useCallback, useEffect, useState } from 'react';
import { BoardBar, BoardLayer } from './components/BoardEditor';
import { UpdateBanner } from './components/Updates';
import { Header, type Tab } from './components/Header';
import { Logo } from './components/Logo';
import { Lock } from './pages/Lock';
import { Coach } from './pages/Coach';
import { Settings } from './pages/Settings';
import { Training, type TrainingSub } from './pages/Training';
import { Health } from './pages/Health';
import { Today } from './pages/Today';
import { Nutrition } from './pages/Nutrition';
import { Psychology } from './pages/Psychology';
import { api, errText, inTauri, type Status } from './lib/api';
import { scrollToCard, type Go } from './lib/nav';
import { Help } from './pages/Help';
import { HelpLayer } from './components/HelpLayer';
import { Tour } from './components/Tour';
import { HELP_PAGE_EVENT } from './lib/helptips';

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [fatal, setFatal] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>('today');
  const [trainSub, setTrainSub] = useState<TrainingSub>('overview');
  const goImport = () => { setTrainSub('io'); setTab('training'); };
  const [workout, setWorkout] = useState<string | null>(null);
  const [backTo, setBackTo] = useState<Tab | null>(null);
  const [backDay, setBackDay] = useState<string | null>(null);
  const openWorkout = (id: string, day?: string) => { setWorkout(id); setBackTo(tab !== 'training' ? tab : null); setBackDay(day ?? null); setTab('training'); };
  const BACK_NAME: Partial<Record<Tab, string>> = { health: 'Здоровье', psychology: 'Психология', today: 'Сегодня', coach: 'Тренер', nutrition: 'Питание' };
  const [healthDay, setHealthDay] = useState<string | null>(null);
  const [healthSub, setHealthSub] = useState<'overview' | 'history' | 'meds' | 'labs' | null>(null);
  const [coachSub, setCoachSub] = useState<string | null>(null);
  const [helpFocus, setHelpFocus] = useState<string | null>(null);
  useEffect(() => {
    const on = (e: Event) => { setHelpFocus((e as CustomEvent<string | null>).detail); setTab('help'); };
    window.addEventListener(HELP_PAGE_EVENT, on);
    return () => window.removeEventListener(HELP_PAGE_EVENT, on);
  }, []);
  const [setSub, setSetSub] = useState<'prof' | null>(null);
  const clearWorkout = useCallback(() => setWorkout(null), []);
  /** One way for links to lead somewhere: tab → subtab → card (1.0). */
  const go = (g: Go) => {
    if (g.tab === 'training') setTrainSub((g.sub as TrainingSub) ?? 'overview');
    if (g.tab === 'health') setHealthSub((g.sub as 'overview' | 'history' | 'meds' | 'labs') ?? null);
    if (g.tab === 'coach') setCoachSub(g.sub ?? null);
    if (g.tab === 'settings') setSetSub(g.sub === 'prof' ? 'prof' : null);
    setTab(g.tab as Tab);
    if (g.card) scrollToCard(g.card);
  };

  const refresh = useCallback(() => {
    api.status().then(setStatus).catch((e) => setFatal(errText(e)));
  }, []);
  useEffect(refresh, [refresh]);

  if (fatal) {
    return (
      <div className="lock"><div className="lock-card"><Logo size={56} /><div className="note-err">{fatal}</div></div></div>
    );
  }
  if (!status) {
    return <div className="lock"><Logo size={56} /></div>;
  }
  if (!status.unlocked) {
    return <Lock onUnlocked={refresh} />;
  }

  return (
    <div className="app">
      <div className="wrap" data-page={tab}>
        <Header tab={tab} onTab={setTab} />
        <UpdateBanner />
        {status.dbError && (
          <div className="note-err" role="alert">
            {status.dbError}
          </div>
        )}
        {tab === 'help' ? (
          <Help focus={helpFocus} onFocusUsed={() => setHelpFocus(null)} />
        ) : tab === 'settings' ? (
          <Settings status={status} refresh={refresh} goImport={goImport} initialSub={setSub} onSubUsed={() => setSetSub(null)} />
        ) : tab === 'training' ? (
          <Training sub={trainSub} onSub={setTrainSub} open={workout} onOpenDone={clearWorkout} back={backTo ? { label: BACK_NAME[backTo] ?? 'Назад', go: () => { const t = backTo; setBackTo(null); if (t === 'health') { if (backDay) setHealthDay(backDay); else setHealthSub('history'); } setTab(t); } } : null} />
        ) : tab === 'health' ? (
          <Health onSettings={() => setTab('settings')} onOpenWorkout={openWorkout} day={healthDay} onDayUsed={() => setHealthDay(null)} initialSub={healthSub} onSubUsed={() => setHealthSub(null)} />
        ) : tab === 'psychology' ? (
          <Psychology onOpenWorkout={openWorkout} />
        ) : tab === 'nutrition' ? (
          <Nutrition onHealth={() => setTab('health')} go={go} onLabs={() => { setHealthSub('labs'); setTab('health'); }} />
        ) : tab === 'coach' ? (
          <Coach initialSub={coachSub} onSubUsed={() => setCoachSub(null)} />
        ) : tab === 'today' ? (
          <Today onSettings={() => setTab('settings')} onTraining={() => { setTrainSub('overview'); setTab('training'); }} onHealth={() => setTab('health')} onPsych={() => setTab('psychology')} onHealthDay={(d) => { setHealthDay(d); setTab('health'); }} onMeds={() => { setHealthSub('meds'); setTab('health'); }} onCoach={() => setTab('coach')} onGear={() => { setTrainSub('gear'); setTab('training'); }} onProfile={() => { setSetSub('prof'); setTab('settings'); }} go={go} />
        ) : null}
        <p className="footer">
          Kouros {status.version}{!inTauri && ' · демо в браузере'}
        </p>
        <BoardLayer />
      </div>
      <BoardBar />
      <HelpLayer />
      <Tour go={go} tab={tab} auto={!!status.unlocked} />
    </div>
  );
}
