// 3D models with three.js (loaded only when a model is opened). Renders on demand, so an
// idle model costs no CPU/GPU.
import type * as THREE_NS from 'three';
import { h } from '../lib/dom';
import { dirUrl, fileUrl } from '../lib/urls';
import type { Mounted, ViewCtx } from './types';

type Three = typeof THREE_NS;

/**
 * One renderer for the whole session. WebKit frees WebGL contexts lazily (if ever), so
 * creating one per preview leaks tens of MB each time; reusing it keeps memory flat.
 */
let shared: { renderer: THREE_NS.WebGLRenderer; env: THREE_NS.Texture } | null = null;

async function sharedRenderer(THREE: Three) {
  if (shared) return shared;
  const { RoomEnvironment } = await import('three/examples/jsm/environments/RoomEnvironment.js');
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'high-performance' });
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.0;
  renderer.domElement.style.cssText = 'display:block;width:100%;height:100%;outline:none;touch-action:none';
  const pmrem = new THREE.PMREMGenerator(renderer);
  const env = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
  pmrem.dispose();
  shared = { renderer, env };
  return shared;
}

async function loadModel(THREE: Three, ctx: ViewCtx): Promise<{ object: THREE_NS.Object3D; animations: THREE_NS.AnimationClip[] }> {
  const { info } = ctx;
  const url = fileUrl(info.path);
  const fmt = info.format;
  const solid = (geo: THREE_NS.BufferGeometry) => {
    // Many exporters write zero normals; recomputing is cheap and always correct.
    if (fmt === 'stl' || !geo.attributes.normal) geo.computeVertexNormals();
    const hasColor = !!geo.attributes.color;
    if (!geo.index && geo.attributes.position && fmt === 'ply' && !geo.attributes.normal) {
      return new THREE.Points(geo, new THREE.PointsMaterial({ size: 0.01, vertexColors: hasColor }));
    }
    return new THREE.Mesh(geo, new THREE.MeshStandardMaterial({ color: hasColor ? 0xffffff : 0xb8bcc8, vertexColors: hasColor, metalness: 0.1, roughness: 0.55, side: THREE.DoubleSide }));
  };
  switch (fmt) {
    case 'glb':
    case 'gltf': {
      const { GLTFLoader } = await import('three/examples/jsm/loaders/GLTFLoader.js');
      const { MeshoptDecoder } = await import('three/examples/jsm/libs/meshopt_decoder.module.js');
      const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder);
      const g = await loader.loadAsync(url);
      return { object: g.scene, animations: g.animations };
    }
    case 'obj': {
      const { OBJLoader } = await import('three/examples/jsm/loaders/OBJLoader.js');
      const loader = new OBJLoader();
      // Honour `mtllib` so models keep their materials and textures.
      try {
        const head = await (await fetch(url, { headers: { Range: 'bytes=0-65535' } })).text();
        const mtl = head.match(/^mtllib\s+(.+?)\s*$/m)?.[1];
        if (mtl && info.dir) {
          const { MTLLoader } = await import('three/examples/jsm/loaders/MTLLoader.js');
          const materials = await new MTLLoader().setPath(dirUrl(info.dir)).loadAsync(mtl);
          materials.preload();
          loader.setMaterials(materials);
        }
      } catch {
        /* materials are optional */
      }
      return { object: await loader.loadAsync(url), animations: [] };
    }
    case 'stl': {
      const { STLLoader } = await import('three/examples/jsm/loaders/STLLoader.js');
      return { object: solid(await new STLLoader().loadAsync(url)), animations: [] };
    }
    case 'ply': {
      const { PLYLoader } = await import('three/examples/jsm/loaders/PLYLoader.js');
      return { object: solid(await new PLYLoader().loadAsync(url)), animations: [] };
    }
    case 'fbx': {
      const { FBXLoader } = await import('three/examples/jsm/loaders/FBXLoader.js');
      const o = await new FBXLoader().loadAsync(url);
      return { object: o, animations: o.animations ?? [] };
    }
    case 'dae': {
      const { ColladaLoader } = await import('three/examples/jsm/loaders/ColladaLoader.js');
      const c = await new ColladaLoader().loadAsync(url);
      if (!c) throw new Error('Could not parse Collada file');
      return { object: c.scene, animations: c.scene.animations ?? [] };
    }
    case '3mf': {
      const { ThreeMFLoader } = await import('three/examples/jsm/loaders/3MFLoader.js');
      return { object: await new ThreeMFLoader().loadAsync(url), animations: [] };
    }
  }
  throw new Error(`Unsupported 3D format: ${fmt}`);
}

export async function mount(host: HTMLElement, ctx: ViewCtx): Promise<Mounted> {
  const THREE = await import('three');
  const { OrbitControls } = await import('three/examples/jsm/controls/OrbitControls.js');

  const { object, animations } = await loadModel(THREE, ctx);
  if (ctx.signal.aborted) throw new DOMException('aborted', 'AbortError');

  const wrap = h('div.model-wrap', { style: 'flex:1;position:relative;min-height:0;overflow:hidden' });
  host.append(wrap);
  let renderer: THREE_NS.WebGLRenderer;
  let env: THREE_NS.Texture;
  try {
    ({ renderer, env } = await sharedRenderer(THREE));
  } catch {
    throw new Error('3D preview needs WebGL, which is unavailable on this system.');
  }
  renderer.setPixelRatio(Math.min(devicePixelRatio || 1, 2));
  wrap.append(renderer.domElement);

  const scene = new THREE.Scene();
  scene.environment = env;
  scene.environmentIntensity = 0.85;
  scene.add(new THREE.HemisphereLight(0xffffff, 0x444455, 0.6));
  const sun = new THREE.DirectionalLight(0xffffff, 1.4);
  sun.position.set(3, 5, 4);
  scene.add(sun);

  // Frame the model: centre it and sit it on the grid.
  const box = new THREE.Box3().setFromObject(object);
  const size = box.getSize(new THREE.Vector3());
  const radius = Math.max(size.length() / 2, 1e-6);
  const center = box.getCenter(new THREE.Vector3());
  object.position.sub(new THREE.Vector3(center.x, box.min.y, center.z));
  scene.add(object);

  const grid = new THREE.GridHelper(radius * 4, 20, 0x888899, 0x888899);
  const gridMat = grid.material as THREE_NS.Material;
  gridMat.transparent = true;
  gridMat.opacity = 0.18;
  scene.add(grid);

  const camera = new THREE.PerspectiveCamera(40, 1, radius / 1000, radius * 1000);
  const target = new THREE.Vector3(0, size.y / 2, 0);
  const home = () => {
    const dist = radius / Math.sin(THREE.MathUtils.degToRad(camera.fov / 2)) * 1.1;
    camera.position.copy(target).add(new THREE.Vector3(1, 0.65, 1.2).normalize().multiplyScalar(dist));
    camera.lookAt(target);
    controls.target.copy(target);
    controls.update();
  };
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 1.2;
  controls.addEventListener('start', () => {
    controls.autoRotate = false;
    rotateBtn?.classList.remove('active');
  });
  home();

  let mixer: THREE_NS.AnimationMixer | null = null;
  if (animations.length) {
    mixer = new THREE.AnimationMixer(object);
    mixer.clipAction(animations[0]).play();
  }

  let dirty = true;
  controls.addEventListener('change', () => (dirty = true));
  const resize = () => {
    const w = wrap.clientWidth;
    const hgt = wrap.clientHeight;
    if (!w || !hgt) return;
    renderer.setSize(w, hgt, false);
    camera.aspect = w / hgt;
    camera.updateProjectionMatrix();
    dirty = true;
  };
  const ro = new ResizeObserver(resize);
  ro.observe(wrap);
  resize();

  let last = performance.now();
  renderer.setAnimationLoop((now: number) => {
    const dt = Math.min(0.1, (now - last) / 1000);
    last = now;
    const moved = controls.update(dt);
    if (mixer) {
      mixer.update(dt);
      dirty = true;
    }
    if (dirty || moved || controls.autoRotate) {
      renderer.render(scene, camera);
      dirty = false;
    }
  });

  // Stats.
  let tris = 0;
  let verts = 0;
  let meshes = 0;
  const materials = new Set<THREE_NS.Material>();
  object.traverse((o) => {
    const m = o as THREE_NS.Mesh;
    if (m.isMesh && m.geometry) {
      meshes++;
      const g = m.geometry;
      const pos = g.attributes.position;
      if (pos) verts += pos.count;
      tris += g.index ? g.index.count / 3 : (pos?.count ?? 0) / 3;
      (Array.isArray(m.material) ? m.material : [m.material]).forEach((x) => materials.add(x));
    }
  });
  ctx.setStatus(`${Math.round(tris).toLocaleString()} triangles`);
  ctx.setDetails([
    ['Triangles', Math.round(tris).toLocaleString()],
    ['Vertices', verts.toLocaleString()],
    ['Meshes', meshes.toLocaleString()],
    ['Materials', materials.size.toLocaleString()],
    ['Animations', animations.length ? animations.map((a) => a.name || 'unnamed').join(', ') : ''],
    ['Size', `${size.x.toPrecision(3)} × ${size.y.toPrecision(3)} × ${size.z.toPrecision(3)}`],
  ]);

  let wire = false;
  const setWire = (on: boolean) => {
    wire = on;
    for (const m of materials) (m as THREE_NS.MeshStandardMaterial).wireframe = on;
    wireBtn?.classList.toggle('active', on);
    dirty = true;
  };
  const toggleRotate = () => {
    controls.autoRotate = !controls.autoRotate;
    rotateBtn?.classList.toggle('active', controls.autoRotate);
  };
  const bar = ctx.toolbar([
    { icon: 'orbit', title: 'Auto-rotate (A)', active: true, onClick: toggleRotate },
    { icon: 'cube', title: 'Wireframe (W)', onClick: () => setWire(!wire) },
    { icon: 'grid', title: 'Grid', active: true, onClick: (b) => { grid.visible = !grid.visible; b.classList.toggle('active', grid.visible); dirty = true; } },
    { icon: 'reset', title: 'Reset view (0)', onClick: () => { home(); dirty = true; } },
  ]);
  const [rotateBtn, wireBtn] = Array.from(bar.querySelectorAll('button'));

  return {
    keydown(e) {
      if (e.ctrlKey || e.metaKey || e.altKey) return false;
      switch (e.key) {
        case 'w': case 'W': setWire(!wire); return true;
        case 'a': case 'A': toggleRotate(); return true;
        case '0': home(); dirty = true; return true;
      }
      return false;
    },
    dispose() {
      renderer.setAnimationLoop(null);
      ro.disconnect();
      controls.dispose();
      object.traverse((o) => {
        const m = o as THREE_NS.Mesh;
        m.geometry?.dispose?.();
        for (const mat of Array.isArray(m.material) ? m.material : m.material ? [m.material] : []) {
          for (const v of Object.values(mat)) if (v && (v as THREE_NS.Texture).isTexture) (v as THREE_NS.Texture).dispose();
          mat.dispose();
        }
      });
      grid.geometry.dispose();
      gridMat.dispose();
      renderer.renderLists.dispose();
      renderer.domElement.remove(); // the renderer itself is reused by the next preview
    },
  };
}
