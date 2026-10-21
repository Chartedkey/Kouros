import { useCallback, useEffect, useState } from 'react';
import { BoardBar, BoardLayer } from './components/BoardEditor';
import { UpdateBanner } from './components/Updates';
import { Header, type Tab } from './components/Header';
import { Logo } from './components/Logo';
import { Lock } from './pages/Lock';
import { Placeholder } from './pages/Placeholder';
import { Coach } from './pages/Coach';
import { Settings } from './pages/Settings';
import { Training, type TrainingSub } from './pages/Training';
import { Health } from './pages/Health';
import { Today } from './pages/Today';
import { Nutrition } from './pages/Nutrition';
import { Psychology } from './pages/Psychology';
import { api, errText, inTauri, type Status } from './lib/api';

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [fatal, setFatal] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>('today');
  const [trainSub, setTrainSub] = useState<TrainingSub>('overview');
  const goImport = () => { setTrainSub('io'); setTab('training'); };
  const [workout, setWorkout] = useState<string | null>(null);
  const openWorkout = (id: string) => { setWorkout(id); setTab('training'); };
  const [healthDay, setHealthDay] = useState<string | null>(null);
  const [healthSub, setHealthSub] = useState<'meds' | 'labs' | null>(null);
  const [setSub, setSetSub] = useState<'prof' | null>(null);
  const clearWorkout = useCallback(() => setWorkout(null), []);

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
        {tab === 'settings' ? (
          <Settings status={status} refresh={refresh} goImport={goImport} initialSub={setSub} onSubUsed={() => setSetSub(null)} />
        ) : tab === 'training' ? (
          <Training sub={trainSub} onSub={setTrainSub} open={workout} onOpenDone={clearWorkout} />
        ) : tab === 'health' ? (
          <Health onSettings={() => setTab('settings')} onOpenWorkout={openWorkout} day={healthDay} onDayUsed={() => setHealthDay(null)} initialSub={healthSub} onSubUsed={() => setHealthSub(null)} />
        ) : tab === 'psychology' ? (
          <Psychology onOpenWorkout={openWorkout} />
        ) : tab === 'nutrition' ? (
          <Nutrition onHealth={() => setTab('health')} onLabs={() => { setHealthSub('labs'); setTab('health'); }} />
        ) : tab === 'coach' ? (
          <Coach />
        ) : tab === 'today' ? (
          <Today onSettings={() => setTab('settings')} onTraining={() => { setTrainSub('overview'); setTab('training'); }} onHealth={() => setTab('health')} onPsych={() => setTab('psychology')} onHealthDay={(d) => { setHealthDay(d); setTab('health'); }} onMeds={() => { setHealthSub('meds'); setTab('health'); }} onCoach={() => setTab('coach')} onProfile={() => { setSetSub('prof'); setTab('settings'); }} />
        ) : (
          <Placeholder tab={tab} onSettings={() => setTab('settings')} />
        )}
        <p className="footer">
          Kouros {status.version}{!inTauri && ' · демо в браузере'}
        </p>
        <BoardLayer />
      </div>
      <BoardBar />
    </div>
  );
}
