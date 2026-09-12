import React, { useEffect, useRef } from 'react';
import * as THREE from 'three';
import type { DaemonStatus, AudioLevels } from '../types';

interface OrbCanvasProps {
  status: DaemonStatus;
  audioLevels: AudioLevels;
  className?: string;
}

// Generate a soft circular glow texture for particles without needing external image assets
function createParticleTexture(): THREE.Texture {
  const canvas = document.createElement('canvas');
  canvas.width = 64;
  canvas.height = 64;
  const ctx = canvas.getContext('2d');
  if (ctx) {
    const gradient = ctx.createRadialGradient(32, 32, 0, 32, 32, 32);
    gradient.addColorStop(0, 'rgba(255, 255, 255, 1)');
    gradient.addColorStop(0.2, 'rgba(255, 255, 255, 0.85)');
    gradient.addColorStop(0.5, 'rgba(255, 255, 255, 0.3)');
    gradient.addColorStop(1, 'rgba(255, 255, 255, 0)');

    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, 64, 64);
  }
  const texture = new THREE.CanvasTexture(canvas);
  texture.needsUpdate = true;
  return texture;
}

export const OrbCanvas: React.FC<OrbCanvasProps> = ({ status, audioLevels, className = '' }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const statusRef = useRef(status);
  const audioLevelsRef = useRef(audioLevels);

  // Keep refs in sync for requestAnimationFrame loop
  useEffect(() => {
    statusRef.current = status;
  }, [status]);

  useEffect(() => {
    audioLevelsRef.current = audioLevels;
  }, [audioLevels]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const width = container.clientWidth || 400;
    const height = container.clientHeight || 400;

    // Scene setup
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(50, width / height, 0.1, 1000);
    camera.position.z = 5.2;

    const renderer = new THREE.WebGLRenderer({
      antialias: true,
      alpha: true,
      powerPreference: 'high-performance',
    });
    renderer.setSize(width, height);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    // Particle Texture
    const particleTexture = createParticleTexture();

    // 1. Core Sphere Particles (~14,000 points)
    const particleCount = 14000;
    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(particleCount * 3);
    const basePositions = new Float32Array(particleCount * 3);
    const colors = new Float32Array(particleCount * 3);
    const scales = new Float32Array(particleCount);
    const speeds = new Float32Array(particleCount);

    // Fibonacci sphere distribution for uniform spherical coverage + noise
    const radius = 1.85;
    for (let i = 0; i < particleCount; i++) {
      const phi = Math.acos(1 - 2 * (i + 0.5) / particleCount);
      const theta = Math.PI * (1 + Math.sqrt(5)) * i;

      // Slight radial variation to give depth
      const r = radius * (0.85 + 0.3 * Math.random());
      const x = r * Math.sin(phi) * Math.cos(theta);
      const y = r * Math.sin(phi) * Math.sin(theta);
      const z = r * Math.cos(phi);

      positions[i * 3] = x;
      positions[i * 3 + 1] = y;
      positions[i * 3 + 2] = z;

      basePositions[i * 3] = x;
      basePositions[i * 3 + 1] = y;
      basePositions[i * 3 + 2] = z;

      // Initial color cyan/blue
      colors[i * 3] = 0.05;
      colors[i * 3 + 1] = 0.7;
      colors[i * 3 + 2] = 0.95;

      scales[i] = Math.random() * 0.8 + 0.4;
      speeds[i] = Math.random() * 1.5 + 0.5;
    }

    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));

    const material = new THREE.PointsMaterial({
      size: 0.075,
      map: particleTexture,
      vertexColors: true,
      transparent: true,
      opacity: 0.9,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    const particleSystem = new THREE.Points(geometry, material);
    scene.add(particleSystem);

    // 2. Outer Halo Ring (~3,000 points)
    const ringCount = 3000;
    const ringGeometry = new THREE.BufferGeometry();
    const ringPositions = new Float32Array(ringCount * 3);
    const ringColors = new Float32Array(ringCount * 3);

    for (let i = 0; i < ringCount; i++) {
      const angle = (i / ringCount) * Math.PI * 2;
      const ringR = 2.4 + (Math.random() - 0.5) * 0.4;
      ringPositions[i * 3] = Math.cos(angle) * ringR;
      ringPositions[i * 3 + 1] = (Math.random() - 0.5) * 0.35;
      ringPositions[i * 3 + 2] = Math.sin(angle) * ringR;

      ringColors[i * 3] = 0.1;
      ringColors[i * 3 + 1] = 0.5;
      ringColors[i * 3 + 2] = 0.8;
    }

    ringGeometry.setAttribute('position', new THREE.BufferAttribute(ringPositions, 3));
    ringGeometry.setAttribute('color', new THREE.BufferAttribute(ringColors, 3));

    const ringMaterial = new THREE.PointsMaterial({
      size: 0.06,
      map: particleTexture,
      vertexColors: true,
      transparent: true,
      opacity: 0.6,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });

    const ringSystem = new THREE.Points(ringGeometry, ringMaterial);
    ringSystem.rotation.x = 0.45;
    ringSystem.rotation.z = 0.2;
    scene.add(ringSystem);

    // 3. Inner Glowing Core
    const coreGeometry = new THREE.SphereGeometry(0.7, 32, 32);
    const coreMaterial = new THREE.MeshBasicMaterial({
      color: 0x00d2ff,
      transparent: true,
      opacity: 0.2,
      wireframe: true,
    });
    const coreMesh = new THREE.Mesh(coreGeometry, coreMaterial);
    scene.add(coreMesh);

    // Mouse parallax tracking
    let mouseX = 0;
    let mouseY = 0;
    let targetMouseX = 0;
    let targetMouseY = 0;

    const handleMouseMove = (e: MouseEvent) => {
      const rect = container.getBoundingClientRect();
      const x = (e.clientX - rect.left) / rect.width - 0.5;
      const y = (e.clientY - rect.top) / rect.height - 0.5;
      targetMouseX = x * 0.8;
      targetMouseY = y * 0.8;
    };

    window.addEventListener('mousemove', handleMouseMove);

    // Color interpolation state
    const currentColor = { r: 0.05, g: 0.7, b: 0.95 };
    const targetColor = { r: 0.05, g: 0.7, b: 0.95 };
    const altColor = { r: 0.0, g: 0.4, b: 0.8 };

    // Audio smoothed values
    let smoothedRms = 0;
    let smoothedPeak = 0;

    // Animation loop
    let lastTime = performance.now();
    const startTime = performance.now();
    let animId: number;

    const animate = () => {
      animId = requestAnimationFrame(animate);

      const now = performance.now();
      const delta = Math.min((now - lastTime) / 1000, 0.1);
      lastTime = now;
      const time = (now - startTime) / 1000;

      const curStatus = statusRef.current;
      const { rms, peak } = audioLevelsRef.current;

      // Smooth audio levels (fast attack, gentle decay)
      smoothedRms += (rms - smoothedRms) * 0.22;
      smoothedPeak += (peak - smoothedPeak) * 0.3;

      // When speaking or listening, if audio_levels are not yet streamed by hardware audio daemon,
      // synthesize realistic organic speech/listening amplitude based on active state.
      const effectiveRms =
        curStatus === 'speaking' && smoothedRms < 0.05
          ? 0.35 + 0.2 * Math.sin(time * 6.5) + 0.15 * Math.cos(time * 12.0)
          : curStatus === 'listening' && smoothedRms < 0.05
          ? 0.18 + 0.12 * Math.sin(time * 4.0)
          : smoothedRms;

      const effectivePeak =
        curStatus === 'speaking' && smoothedPeak < 0.08
          ? Math.max(0.4, effectiveRms * 1.4 + 0.2 * Math.sin(time * 9.0))
          : smoothedPeak;

      // Mouse smoothing
      mouseX += (targetMouseX - mouseX) * 0.05;
      mouseY += (targetMouseY - mouseY) * 0.05;

      // Status-specific behaviors
      let rotationSpeed = 0.35;
      let waveFrequency = 2.0;
      let waveAmplitude = 0.12;

      switch (curStatus) {
        case 'listening':
          // Cyan / Blue pulsing reactive to user voice
          targetColor.r = 0.0;
          targetColor.g = 0.85;
          targetColor.b = 1.0;
          altColor.r = 0.1;
          altColor.g = 0.4;
          altColor.b = 0.9;
          rotationSpeed = 0.5 + effectiveRms * 1.5;
          waveFrequency = 3.5;
          waveAmplitude = 0.18 + effectiveRms * 0.65;
          material.size = 0.08 + effectivePeak * 0.05;
          material.opacity = 0.95;
          coreMaterial.color.setHex(0x00e5ff);
          coreMaterial.opacity = 0.25 + effectiveRms * 0.4;
          break;

        case 'thinking':
          // Violet / Ambre rapid spinning swirl
          targetColor.r = 0.65;
          targetColor.g = 0.3;
          targetColor.b = 1.0;
          altColor.r = 0.95;
          altColor.g = 0.6;
          altColor.b = 0.1;
          rotationSpeed = 2.4; // rapid rotation
          waveFrequency = 5.0;
          waveAmplitude = 0.25 + Math.sin(time * 6) * 0.1;
          material.size = 0.085;
          material.opacity = 0.92;
          coreMaterial.color.setHex(0xa855f7);
          coreMaterial.opacity = 0.35;
          break;

        case 'speaking':
          // Warm gold / radiant amber / pure white bursts with Kokoro voice audio
          targetColor.r = 1.0;
          targetColor.g = 0.78;
          targetColor.b = 0.2;
          altColor.r = 1.0;
          altColor.g = 0.95;
          altColor.b = 0.85;
          rotationSpeed = 0.8 + effectiveRms * 1.2;
          waveFrequency = 4.0;
          waveAmplitude = 0.2 + effectiveRms * 0.85;
          material.size = 0.085 + effectivePeak * 0.07;
          material.opacity = 0.95;
          coreMaterial.color.setHex(0xfbbf24);
          coreMaterial.opacity = 0.3 + effectiveRms * 0.5;
          break;

        case 'idle':
        default:
          // Attenuated slow pulsing blue/cyan
          targetColor.r = 0.12;
          targetColor.g = 0.55;
          targetColor.b = 0.85;
          altColor.r = 0.05;
          altColor.g = 0.25;
          altColor.b = 0.6;
          rotationSpeed = 0.25;
          waveFrequency = 1.5;
          waveAmplitude = 0.08;
          material.size = 0.07;
          material.opacity = 0.8;
          coreMaterial.color.setHex(0x0284c7);
          coreMaterial.opacity = 0.12;
          break;
      }

      // Smooth color interpolation
      currentColor.r += (targetColor.r - currentColor.r) * 0.08;
      currentColor.g += (targetColor.g - currentColor.g) * 0.08;
      currentColor.b += (targetColor.b - currentColor.b) * 0.08;

      // Base rotation + mouse tilt
      particleSystem.rotation.y += rotationSpeed * delta;
      particleSystem.rotation.x = mouseY * 0.5 + Math.sin(time * 0.5) * 0.08;
      particleSystem.rotation.z = mouseX * 0.5;

      ringSystem.rotation.y -= (rotationSpeed * 0.7) * delta;
      ringSystem.rotation.x = 0.45 + mouseY * 0.3;

      coreMesh.rotation.y += (rotationSpeed * 0.5) * delta;
      const coreScale = 1 + smoothedRms * 0.5;
      coreMesh.scale.set(coreScale, coreScale, coreScale);

      // Animate sphere particle positions based on wave distortion and audio amplitude
      const posAttr = geometry.attributes.position as THREE.BufferAttribute;
      const colAttr = geometry.attributes.color as THREE.BufferAttribute;
      const posArray = posAttr.array as Float32Array;
      const colArray = colAttr.array as Float32Array;

      for (let i = 0; i < particleCount; i++) {
        const i3 = i * 3;
        const bx = basePositions[i3];
        const by = basePositions[i3 + 1];
        const bz = basePositions[i3 + 2];

        // Spherical coordinates angles
        const norm = Math.sqrt(bx * bx + by * by + bz * bz);
        const nx = bx / norm;
        const ny = by / norm;
        const nz = bz / norm;

        // 3D dynamic undulation
        const wave =
          Math.sin(nx * waveFrequency + time * 2.5) *
          Math.cos(ny * waveFrequency + time * 2.0) *
          Math.sin(nz * waveFrequency + time * 1.8);

        // Displacement magnitude: base wave + audio reaction
        const displacement = 1.0 + wave * waveAmplitude + effectiveRms * 0.35 + (Math.random() - 0.5) * effectivePeak * 0.15;

        posArray[i3] = bx * displacement;
        posArray[i3 + 1] = by * displacement;
        posArray[i3 + 2] = bz * displacement;

        // Dynamic particle colors (blend between main state color and secondary highlight)
        const blend = (ny + 1) * 0.5; // gradient top to bottom
        const flash = effectivePeak > 0.5 && curStatus === 'speaking' ? (effectivePeak - 0.5) * 2 : 0;

        colArray[i3] = THREE.MathUtils.lerp(currentColor.r, altColor.r, blend) + flash * 0.5;
        colArray[i3 + 1] = THREE.MathUtils.lerp(currentColor.g, altColor.g, blend) + flash * 0.5;
        colArray[i3 + 2] = THREE.MathUtils.lerp(currentColor.b, altColor.b, blend) + flash * 0.5;
      }

      posAttr.needsUpdate = true;
      colAttr.needsUpdate = true;

      // Update ring particle colors
      const ringColAttr = ringGeometry.attributes.color as THREE.BufferAttribute;
      const ringColArray = ringColAttr.array as Float32Array;
      for (let i = 0; i < ringCount; i++) {
        const i3 = i * 3;
        ringColArray[i3] = currentColor.r * 0.9;
        ringColArray[i3 + 1] = currentColor.g * 0.9;
        ringColArray[i3 + 2] = currentColor.b * 0.9;
      }
      ringColAttr.needsUpdate = true;

      renderer.render(scene, camera);
    };

    animate();

    // Resize handler
    const handleResize = () => {
      if (!container) return;
      const w = container.clientWidth;
      const h = container.clientHeight;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
    };

    window.addEventListener('resize', handleResize);

    return () => {
      window.removeEventListener('resize', handleResize);
      window.removeEventListener('mousemove', handleMouseMove);
      cancelAnimationFrame(animId);
      renderer.dispose();
      geometry.dispose();
      material.dispose();
      ringGeometry.dispose();
      ringMaterial.dispose();
      coreGeometry.dispose();
      coreMaterial.dispose();
      particleTexture.dispose();
      if (container.contains(renderer.domElement)) {
        container.removeChild(renderer.domElement);
      }
    };
  }, []);

  return (
    <div
      ref={containerRef}
      className={`relative w-full h-full flex items-center justify-center overflow-hidden select-none pointer-events-none ${className}`}
    />
  );
};
