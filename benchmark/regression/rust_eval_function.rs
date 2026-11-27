// Real: a color transform's eval method, not JS eval().
impl Transform {
    pub fn eval(&self, input: &[f32], out: &mut [f32]) -> Result<(), CmsError> {
        self.transform.eval(input, out)?;
        Ok(())
    }
}
