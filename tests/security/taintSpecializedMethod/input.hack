final class Unsafe {
    public function isUnsafe() {
        return HH\global_get('_GET')["unsafe"];
    }
}
$a = new Unsafe();
echo $a->isUnsafe();