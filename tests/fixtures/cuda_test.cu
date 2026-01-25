#include <stdio.h>
#include <cuda_runtime.h>

__global__ void vectorAdd(float *a, float *b, float *c, int n) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;  // BREAKPOINT: kernel_entry
    if (idx < n) {
        float val_a = a[idx];  // BREAKPOINT: kernel_compute
        float val_b = b[idx];
        c[idx] = val_a + val_b;
    }
}

int main() {
    printf("CUDA Test Program\n");  // BREAKPOINT: main_start
    
    const int N = 256;
    float *h_a = (float*)malloc(N * sizeof(float));
    float *h_b = (float*)malloc(N * sizeof(float));
    float *h_c = (float*)malloc(N * sizeof(float));
    
    for (int i = 0; i < N; i++) {  // BREAKPOINT: init_loop
        h_a[i] = (float)i;
        h_b[i] = (float)(i * 2);
    }
    
    float *d_a, *d_b, *d_c;
    cudaMalloc(&d_a, N * sizeof(float));
    cudaMalloc(&d_b, N * sizeof(float));
    cudaMalloc(&d_c, N * sizeof(float));
    
    cudaMemcpy(d_a, h_a, N * sizeof(float), cudaMemcpyHostToDevice);
    cudaMemcpy(d_b, h_b, N * sizeof(float), cudaMemcpyHostToDevice);
    
    printf("Launching kernel\n");  // BREAKPOINT: before_kernel
    vectorAdd<<<1, 256>>>(d_a, d_b, d_c, N);
    cudaDeviceSynchronize();
    
    cudaMemcpy(h_c, d_c, N * sizeof(float), cudaMemcpyDeviceToHost);  // BREAKPOINT: after_kernel
    
    printf("Result[0] = %f (expected 0)\n", h_c[0]);
    printf("Result[100] = %f (expected 300)\n", h_c[100]);
    
    cudaFree(d_a); cudaFree(d_b); cudaFree(d_c);
    free(h_a); free(h_b); free(h_c);
    
    printf("Done\n");  // BREAKPOINT: main_end
    return 0;
}
