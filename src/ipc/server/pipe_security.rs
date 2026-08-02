struct PipeSecurity {
    descriptor: SECURITY_DESCRIPTOR,
    attributes: SECURITY_ATTRIBUTES,
}

impl PipeSecurity {
    fn new() -> io::Result<Self> {
        let mut descriptor = unsafe { std::mem::zeroed::<SECURITY_DESCRIPTOR>() };
        let initialized = unsafe {
            InitializeSecurityDescriptor(
                (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                SECURITY_DESCRIPTOR_REVISION,
            )
        };
        if initialized == 0 {
            return Err(io::Error::last_os_error());
        }

        let dacl_set = unsafe {
            SetSecurityDescriptorDacl(
                (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                1,
                null_mut(),
                0,
            )
        };
        if dacl_set == 0 {
            return Err(io::Error::last_os_error());
        }

        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
            bInheritHandle: 0,
        };

        Ok(Self {
            descriptor,
            attributes,
        })
    }

    fn as_mut_ptr(&mut self) -> *mut SECURITY_ATTRIBUTES {
        self.attributes.lpSecurityDescriptor =
            (&mut self.descriptor as *mut SECURITY_DESCRIPTOR).cast();
        &mut self.attributes
    }
}

fn connect_pipe(pipe: &PipeHandle) -> bool {
    let ok = unsafe { ConnectNamedPipe(pipe.raw(), null_mut()) };
    ok != 0 || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED
}

