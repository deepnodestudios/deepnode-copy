import React, { useEffect, useRef } from "react";

interface SpeedChartProps {
  speedHistory: number[]; // in MB/s
  maxSpeed?: number;
}

export const SpeedChart: React.FC<SpeedChartProps> = ({ speedHistory, maxSpeed }) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;

    ctx.clearRect(0, 0, width, height);

    // Background grid
    ctx.strokeStyle = "rgba(255, 255, 255, 0.05)";
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let y = 10; y < height; y += 20) {
      ctx.moveTo(0, y);
      ctx.lineTo(width, y);
    }
    ctx.stroke();

    if (speedHistory.length < 2) return;

    const currentMax = maxSpeed && maxSpeed > 0
      ? maxSpeed
      : Math.max(...speedHistory, 50);

    const stepX = width / (speedHistory.length - 1);

    // Gradient area
    const gradient = ctx.createLinearGradient(0, 0, 0, height);
    gradient.addColorStop(0, "rgba(99, 102, 241, 0.4)");
    gradient.addColorStop(1, "rgba(99, 102, 241, 0.0)");

    ctx.beginPath();
    ctx.moveTo(0, height);

    speedHistory.forEach((speed, idx) => {
      const x = idx * stepX;
      const normalized = Math.min(speed / currentMax, 1);
      const y = height - normalized * (height - 8);
      ctx.lineTo(x, y);
    });

    ctx.lineTo(width, height);
    ctx.closePath();
    ctx.fillStyle = gradient;
    ctx.fill();

    // Line stroke
    ctx.beginPath();
    speedHistory.forEach((speed, idx) => {
      const x = idx * stepX;
      const normalized = Math.min(speed / currentMax, 1);
      const y = height - normalized * (height - 8);
      if (idx === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });

    ctx.strokeStyle = "#818cf8";
    ctx.lineWidth = 2;
    ctx.stroke();
  }, [speedHistory, maxSpeed]);

  return (
    <div className="w-full h-12 bg-neutral-950/70 border border-neutral-800 rounded-md overflow-hidden relative">
      <canvas ref={canvasRef} width={600} height={48} className="w-full h-full block" />
      <div className="absolute top-1 right-2 text-[10px] text-neutral-400 font-mono">
        {speedHistory.length > 0 ? `${speedHistory[speedHistory.length - 1].toFixed(1)} MB/s` : "0.0 MB/s"}
      </div>
    </div>
  );
};
