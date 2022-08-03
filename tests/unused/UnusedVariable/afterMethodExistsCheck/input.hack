class A {
    public function __construct(dict<string, string> $options) {
        $this->setOptions($options);
    }

    protected function setOptions(dict<string, string> $options): void
    {
        foreach ($options as $key => $value) {
            $normalized = ucfirst($key);
            $method     = "set" . $normalized;

            if (method_exists($this, $method)) {
                $this->$method($value);
            }
        }
    }
}

new A(dict["bar" => "bat"]);