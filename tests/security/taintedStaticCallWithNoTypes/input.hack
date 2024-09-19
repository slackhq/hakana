final class A {
    public static function rawinput() {
        return HH\global_get('_GET')['rawinput'];
    }
}

echo A::rawinput();