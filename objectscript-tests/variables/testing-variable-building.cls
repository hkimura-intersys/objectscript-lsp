Class BigClass Extends (%Library.Persistent, %Library.Populate)
{
    Method GetName() As %String
    {
        write "Hello, World!"
        Return ..Name
    }

    Method testingGetDef() [ProcedureBlock=0] {
        set y = y + 2
        w y
    }

    ClassMethod testing() {
        set ytest = 3
        write ytest
    }

    Method newVarChange() {
        set x = 2
        w x
    }

    ClassMethod newPubVar() [publiclist = y] {
        set z = 3
        w z
        w y
    }

    ClassMethod testUpdating() [ProcedureBlock=0] {
        set hannah = 3
        w hannah
        set y = 17
        w y
        set test = 2
        w test
    }

    ClassMethod multiplePubVarDefs() [ProcedureBlock=0] {
        set y = 24
    }

}
