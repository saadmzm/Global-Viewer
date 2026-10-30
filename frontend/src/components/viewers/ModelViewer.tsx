import { Suspense } from 'react'
import { Canvas } from '@react-three/fiber'
import { Center, OrbitControls, useGLTF } from '@react-three/drei'

function Model({ url }: { url: string }) {
  const { scene } = useGLTF(url)
  return <primitive object={scene} />
}

/** Interactive glTF/GLB viewer. Lit manually (no network HDRI) and orbit-controllable. */
export default function ModelViewer({ url }: { url: string }) {
  return (
    <Canvas camera={{ position: [3, 2, 5], fov: 50 }}>
      <ambientLight intensity={0.7} />
      <directionalLight position={[5, 5, 5]} intensity={1.2} />
      <directionalLight position={[-5, -3, -5]} intensity={0.4} />
      <Suspense fallback={null}>
        <Center>
          <Model url={url} />
        </Center>
      </Suspense>
      <OrbitControls makeDefault />
    </Canvas>
  )
}
