class A {
    public string $userId;

    public function __construct() {
        $this->userId = (string) $_GET["user_id"];
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