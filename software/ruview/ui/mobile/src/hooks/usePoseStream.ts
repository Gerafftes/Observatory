import { useEffect } from 'react';
import { wsService } from '@/services/ws.service';
import { usePoseStore } from '@/stores/poseStore';
import { useSettingsStore } from '@/stores/settingsStore';

export interface UsePoseStreamResult {
  connectionStatus: ReturnType<typeof usePoseStore.getState>['connectionStatus'];
  lastFrame: ReturnType<typeof usePoseStore.getState>['lastFrame'];
  isSimulated: boolean;
}

export function usePoseStream(): UsePoseStreamResult {
  const connectionStatus = usePoseStore((state) => state.connectionStatus);
  const lastFrame = usePoseStore((state) => state.lastFrame);
  const isSimulated = usePoseStore((state) => state.isSimulated);
  const serverUrl = useSettingsStore((state) => state.serverUrl);
  const apiToken = useSettingsStore((state) => state.apiToken);

  useEffect(() => {
    const unsubscribe = wsService.subscribe((frame) => {
      usePoseStore.getState().handleFrame(frame);
    });

    // Auto-connect to sensing server on mount
    wsService.connect(serverUrl, apiToken);

    return () => {
      unsubscribe();
    };
  }, [serverUrl, apiToken]);

  return { connectionStatus, lastFrame, isSimulated };
}
