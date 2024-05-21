<<\Hakana\SecurityAnalysis\Specialize()>>
final class Unsafe {
    public function isUnsafe() {
        return $_GET["unsafe"];
    }
}

function stub(): Unsafe { }

echo stub()->isUnsafe();