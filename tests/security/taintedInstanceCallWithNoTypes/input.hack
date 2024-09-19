final class A {
    public function rawinput() {
        return HH\global_get('_GET')['rawinput'];
    }
}

echo (new A())->rawinput();