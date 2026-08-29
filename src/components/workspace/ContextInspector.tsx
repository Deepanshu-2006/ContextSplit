import { useEffect, useState } from "react";
import { X, Server, Clock, AppWindow, MousePointerClick, Globe } from "lucide-react";
import { db, ContextSignal } from "../../services/db";

interface ContextInspectorProps {
  itemId: string;
  onClose: () => void;
}

export function ContextInspector({ itemId, onClose }: ContextInspectorProps) {
  const [signals, setSignals] = useState<ContextSignal[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    async function fetchSignals() {
      try {
        const data = await db.listContextSignals(null, itemId);
        setSignals(data);
      } catch (err) {
        console.error("Failed to fetch context signals", err);
      } finally {
        setLoading(false);
      }
    }
    fetchSignals();
  }, [itemId]);

  const getSignalIcon = (type: string) => {
    switch (type) {
      case 'active_app': return <AppWindow size={14} className="text-blue-500" />;
      case 'user_action': return <MousePointerClick size={14} className="text-purple-500" />;
      case 'url_metadata': return <Globe size={14} className="text-green-500" />;
      default: return <Server size={14} className="text-gray-500" />;
    }
  };

  const getSignalTitle = (type: string) => {
    switch (type) {
      case 'active_app': return 'Source Application';
      case 'user_action': return 'User Action';
      case 'url_metadata': return 'URL Metadata';
      default: return type;
    }
  };

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-black/20 backdrop-blur-sm p-4">
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-2xl max-h-[85vh] flex flex-col overflow-hidden border border-gray-200" onClick={(e) => e.stopPropagation()}>
        
        <div className="flex items-center justify-between px-5 py-4 border-b border-gray-100 bg-gray-50/50">
          <div>
            <h2 className="text-lg font-semibold text-gray-900 tracking-tight">Context Inspector</h2>
            <p className="text-xs text-gray-500 font-mono mt-1">Item: {itemId}</p>
          </div>
          <button 
            onClick={onClose}
            className="p-2 rounded-lg hover:bg-gray-200/50 text-gray-500 transition-colors"
          >
            <X size={18} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-5 bg-gray-50 cursor-default">
          {loading ? (
            <div className="text-center py-10 text-gray-400 text-sm">Loading signals...</div>
          ) : signals.length === 0 ? (
            <div className="text-center py-10 text-gray-400 text-sm bg-white rounded-lg border border-dashed border-gray-300">
              No context signals found for this item.
            </div>
          ) : (
            <div className="space-y-4">
              {signals.map((signal) => {
                let parsedPayload = null;
                try {
                  parsedPayload = JSON.parse(signal.payload);
                } catch {
                  // Ignore parse error
                }

                return (
                  <div key={signal.id} className="bg-white rounded-lg border border-gray-200 overflow-hidden shadow-sm">
                    <div className="flex items-center justify-between px-4 py-2.5 bg-gray-50 border-b border-gray-100">
                      <div className="flex items-center gap-2">
                        {getSignalIcon(signal.signal_type)}
                        <span className="text-sm font-medium text-gray-700">
                          {getSignalTitle(signal.signal_type)}
                        </span>
                      </div>
                      <div className="flex items-center gap-1.5 text-gray-400">
                        <Clock size={12} />
                        <span className="text-xs font-mono">
                          {new Date(signal.created_at).toLocaleTimeString([], { hour12: false })}
                        </span>
                      </div>
                    </div>
                    <div className="p-4">
                      {parsedPayload ? (
                        <div className="grid grid-cols-1 gap-2">
                          {Object.entries(parsedPayload).map(([key, value]) => (
                            <div key={key} className="flex text-sm">
                              <span className="w-32 flex-shrink-0 text-gray-500 font-medium capitalize text-xs pt-0.5">
                                {key.replace(/_/g, ' ')}
                              </span>
                              <span className="text-gray-900 font-mono text-xs break-all bg-gray-50 px-2 py-1 rounded w-full border border-gray-100">
                                {String(value)}
                              </span>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <pre className="text-xs font-mono text-gray-600 bg-gray-50 p-3 rounded overflow-x-auto">
                          {signal.payload}
                        </pre>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
