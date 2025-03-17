/**
 * @module sensitivity
 * 
 * @description
 * 
 * # 模块概述
 * 本模块提供LoRaWAN无线通信中灵敏度和链路预算计算的功能。这些计算对于评估和优化
 * LoRaWAN网络的覆盖范围、可靠性和性能至关重要。
 * 
 * # 文件功能
 * - 计算接收器灵敏度，基于带宽、噪声系数和信噪比
 * - 计算链路预算，评估通信链路的质量和可靠性
 * - 提供标准化的计算方法，用于网络规划和性能分析
 * 
 * # 主要组件
 * - calculate_sensitivity：计算接收器灵敏度的函数
 * - calculate_link_budget：计算链路预算的函数
 * - 测试模块：验证计算的准确性
 * 
 * # 关键流程
 * - 根据香农-哈特利定理计算理论灵敏度
 * - 考虑带宽、噪声系数和所需信噪比的影响
 * - 结合发射功率计算总链路预算
 * 
 * # 重要考虑事项
 * - 灵敏度计算对于确定设备通信范围至关重要
 * - 链路预算影响网络覆盖和可靠性
 * - 不同的扩频因子和带宽设置会影响灵敏度
 * - 这些计算是LoRaWAN网络规划的基础
 */

pub fn calculate_sensitivity(bandwidth_hz: u32, noise_figure: f32, snr: f32) -> f32 {
    // see also: http://www.techplayon.com/lora-link-budget-sensitivity-calculations-example-explained/
    let log_bw = 10.0 * (bandwidth_hz as f32).log10();
    -174.0 + log_bw + (noise_figure + snr)
}

pub fn calculate_link_budget(bandwidth_hz: u32, noise_figure: f32, snr: f32, tx_power: f32) -> f32 {
    tx_power - calculate_sensitivity(bandwidth_hz, noise_figure, snr)
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_sensitivity() {
        let s = calculate_sensitivity(125000, 6.0, -20.0);
        assert_eq!(-137, s as isize);
    }

    #[test]
    fn test_link_budget() {
        let lb = calculate_link_budget(125000, 6.0, -20.0, 17.0);
        assert_eq!(154, lb as isize);
    }
}
