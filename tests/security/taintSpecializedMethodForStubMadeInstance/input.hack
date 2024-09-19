<<\Hakana\SecurityAnalysis\Specialize()>>
final class Unsafe {
    public function isUnsafe() {
        return HH\global_get('_GET')["unsafe"];
    }
}

function stub(): Unsafe { }

echo stub()->isUnsafe();