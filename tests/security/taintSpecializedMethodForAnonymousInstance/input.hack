final class Unsafe {
    public function isUnsafe() {
        return HH\global_get('_GET')["unsafe"];
    }
}
echo (new Unsafe())->isUnsafe();