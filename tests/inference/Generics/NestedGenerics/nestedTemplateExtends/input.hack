namespace Foo;

interface IBaseViewData {}

abstract class BaseModel<TViewData> {}

abstract class BaseRepository<TViewData as IBaseViewData, TModel as BaseModel<TViewData>> {}

class StudentViewData implements IBaseViewData {}
class TeacherViewData implements IBaseViewData {}

class StudentModel extends BaseModel<StudentViewData> {}
class TeacherModel extends BaseModel<TeacherViewData> {}

class StudentRepository extends BaseRepository<StudentViewData, StudentModel> {}
class TeacherRepository extends BaseRepository<TeacherViewData, TeacherModel>{}