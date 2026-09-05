use super::UiRenderer;

impl UiRenderer {
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        self.draw_range(pass, 0..self.vertex_count);
    }
    pub fn draw_viewmodel<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        self.gpu_viewmodel.draw(
            pass,
            &self.viewmodel_pipeline,
            &self.icon_bind_group,
            &self.hand_bind_group,
            self.viewmodel_item,
        );
    }
    pub fn draw_ui<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        self.draw_range(pass, self.viewmodel_count..self.vertex_count);
    }
    fn draw_range<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, range: std::ops::Range<u32>) {
        self.draw_range_with_pipeline(pass, range, &self.pipeline);
    }
    fn draw_range_with_pipeline<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        range: std::ops::Range<u32>,
        pipeline: &'a wgpu::RenderPipeline,
    ) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.icon_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(range, 0..1);
    }
}
