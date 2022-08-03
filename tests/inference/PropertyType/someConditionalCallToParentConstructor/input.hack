class GrandParentClassDoesNotDefine {
    public function __construct() {}
}

class ParentClassDefinesVar extends GrandParentClassDoesNotDefine {
    protected string $val;

    public function __construct() {
        $this->val = "hello";
        if (rand(0, 1)) {
            parent::__construct();
        }
    }
}

class ChildClass extends ParentClassDefinesVar {
    public function __construct() {
        parent::__construct();
    }
}