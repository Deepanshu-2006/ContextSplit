import { useState, useEffect, RefObject } from 'react';

export type ProximityState = 'far' | 'near' | 'very_near' | 'active';

interface ProximityOptions {
  nearDistance?: number;
  veryNearDistance?: number;
  activeDistance?: number;
}

export function useCursorProximity(
  targetRef: RefObject<HTMLElement | null>,
  options: ProximityOptions = {}
) {
  const [proximity, setProximity] = useState<ProximityState>('far');
  
  // Default thresholds that feel right for a desktop drop target
  const {
    nearDistance = 400,
    veryNearDistance = 150,
    activeDistance = 60,
  } = options;

  useEffect(() => {
    const handleMove = (e: MouseEvent | DragEvent) => {
      if (!targetRef.current) return;
      
      const rect = targetRef.current.getBoundingClientRect();
      const targetX = rect.left + rect.width / 2;
      const targetY = rect.top + rect.height / 2;
      
      const distance = Math.sqrt(
        Math.pow(e.clientX - targetX, 2) + Math.pow(e.clientY - targetY, 2)
      );

      if (distance <= activeDistance) {
        setProximity('active');
      } else if (distance <= veryNearDistance) {
        setProximity('very_near');
      } else if (distance <= nearDistance) {
        setProximity('near');
      } else {
        setProximity('far');
      }
    };

    // Listen to both dragover and mousemove to allow easy testing and actual usage
    window.addEventListener('dragover', handleMove);
    window.addEventListener('mousemove', handleMove);

    return () => {
      window.removeEventListener('dragover', handleMove);
      window.removeEventListener('mousemove', handleMove);
    };
  }, [targetRef, nearDistance, veryNearDistance, activeDistance]);

  return proximity;
}
