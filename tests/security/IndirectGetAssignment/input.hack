class InputFilter {
    public string $name;

    public function __construct(string $name) {
        $this->name = $name;
    }

    <<\Hakana\SecurityAnalysis\SpecializeCall()>>
    public function getArg(string $method, string $type)
    {
        $arg = null;

        switch ($method) {
            case "post":
                if (isset($_POST[$this->name])) {
                    $arg = $_POST[$this->name];
                }
                break;

            case "get":
                if (isset($_GET[$this->name])) {
                    $arg = $_GET[$this->name];
                }
                break;
        }

        return $this->filterInput($type, $arg);
    }

    protected function filterInput(string $type, $arg)
    {
        // input is null
        if ($arg === null) {
            return null;
        }

        // set to null if sanitize clears arg
        if ($arg === "") {
            $arg = null;
        }

        // type casting
        if ($arg !== null) {
            $arg = $this->typeCastInput($type, $arg);
        }

        return $arg;
    }

    protected function typeCastInput(string $type, $arg) {
        if ($type === "string") {
            return (string) $arg;
        }

        return null;
    }
}

echo (new InputFilter("hello"))->getArg("get", "string");