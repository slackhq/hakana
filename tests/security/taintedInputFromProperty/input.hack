final class A {
    public string $userId;

    public function __construct(string $key) {
        $this->userId = (string) HH\global_get('_GET')[$key];
    }

    public function getAppendedUserId() : string {
        return "aaaa" . $this->userId;
    }

    public function doDelete(AsyncMysqlConnection $conn) : void {
        $userId = $this->getAppendedUserId();
        $this->deleteUser($conn, $userId);
    }

    public function deleteUser(AsyncMysqlConnection $conn, string $userId) : void {
        $conn->query("delete from users where user_id = " . $userId);
    }
}

function foo(AsyncMysqlConnection $conn): void {
    $a = new A("foo");
    $a->doDelete($conn);
}